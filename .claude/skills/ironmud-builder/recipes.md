# Recipe Editor (recedit)

The recipe editor creates crafting and cooking formulas. Recipes define what ingredients and tools are needed to create items.

## Core Concepts

### Recipe Components
- **Skill** - Which skill uses this recipe (cooking or crafting)
- **Level** - Minimum skill level required
- **Ingredients** - Items consumed when crafting
- **Tools** - Items required but not consumed
- **Output** - The item(s) produced

### Ingredient Types
- **By VNUM** - Requires a specific item (e.g., `material:flour`)
- **By Category** - Requires any item with that category (e.g., `@flour`)
- **Liquid** - Requires liquid from a container (e.g., `@liquid:water`)

### Tool Locations
- **Inventory** - Tool must be in player's inventory
- **Room** - Tool must be in the room (e.g., oven, forge)
- **Either** - Tool can be in inventory or room

## Commands

### Creating and Viewing
```
recedit create <vnum>           - Create new recipe
recedit <vnum>                  - Show recipe properties
```

### Basic Properties
```
recedit <vnum> name <text>      - Set display name
recedit <vnum> vnum <new_vnum>  - Change vnum
recedit <vnum> skill <cooking|crafting>
recedit <vnum> level <0-10>     - Skill level required
recedit <vnum> autolearn [on|off] - Auto-learn at skill level?
recedit <vnum> difficulty <1-10>  - How hard to succeed
recedit <vnum> xp <amount>      - Base XP for crafting
recedit <vnum> output <item_vnum> [quantity]
```

### Ingredients
```
recedit <vnum> ingredient list
recedit <vnum> ingredient add <vnum|@category> <quantity>
recedit <vnum> ingredient remove <index>
```

### Tools
```
recedit <vnum> tool list
recedit <vnum> tool add <vnum|@category> <inv|room|either>
recedit <vnum> tool remove <index>
```

## Examples

### Simple Bread Recipe
```
# Create the recipe
recedit create food:bread

# Set basic properties
recedit food:bread name "Simple Bread"
recedit food:bread skill cooking
recedit food:bread level 1
recedit food:bread difficulty 2
recedit food:bread xp 5

# Set output
recedit food:bread output food:loaf_of_bread 1

# Add ingredients
recedit food:bread ingredient add @flour 2
recedit food:bread ingredient add @liquid:water 5

# Require an oven in the room
recedit food:bread tool add @oven room
```

### Healing Potion Recipe
```
recedit create alchemy:heal_minor

recedit alchemy:heal_minor name "Minor Healing Potion"
recedit alchemy:heal_minor skill crafting
recedit alchemy:heal_minor level 3
recedit alchemy:heal_minor difficulty 4
recedit alchemy:heal_minor xp 15
recedit alchemy:heal_minor autolearn off

recedit alchemy:heal_minor output potions:heal_minor 1

# Specific herbs required
recedit alchemy:heal_minor ingredient add herb:bloodmoss 2
recedit alchemy:heal_minor ingredient add herb:ginseng 1
recedit alchemy:heal_minor ingredient add @liquid:water 10

# Needs mortar and pestle in inventory
recedit alchemy:heal_minor tool add @mortar_pestle inv
```

### Iron Sword Recipe
```
recedit create smith:iron_sword

recedit smith:iron_sword name "Iron Sword"
recedit smith:iron_sword skill crafting
recedit smith:iron_sword level 5
recedit smith:iron_sword difficulty 6
recedit smith:iron_sword xp 25

recedit smith:iron_sword output weapon:iron_sword 1

# Raw materials
recedit smith:iron_sword ingredient add material:iron_ingot 3
recedit smith:iron_sword ingredient add material:leather_strip 1

# Smithing requires tools in room
recedit smith:iron_sword tool add furniture:forge room
recedit smith:iron_sword tool add furniture:anvil room

# Hammer can be in inventory or room
recedit smith:iron_sword tool add @hammer either
```

## Ingredient Syntax

### Specific Items
Use the item's vnum directly:
```
recedit <vnum> ingredient add material:flour 2
```
Requires exactly 2 of that specific item.

### Category Matching
Prefix with `@` to match any item with that category:
```
recedit <vnum> ingredient add @flour 2
```
Requires 2 of any item categorized as "flour".

### Liquid Ingredients
Use `@liquid:type` syntax:
```
recedit <vnum> ingredient add @liquid:water 10
```
Requires 10 units (sips) of water from any liquid container.

**Common liquid types:** water, ale, wine, beer, milk, juice, tea, coffee, oil

## Tool Syntax

### Specific Tool
```
recedit <vnum> tool add furniture:forge room
```

### Category Tool
```
recedit <vnum> tool add @knife inv
```
Any item categorized as "knife" in inventory.

### Location Options
- `inv` or `inventory` - Must be in player's inventory
- `room` - Must be in the room
- `either` - Can be in inventory or room

## Best Practices

1. **Skill Organization**
   - Cooking: Food, drinks, consumables
   - Crafting: Weapons, armor, tools, potions

2. **Difficulty Guidelines**
   - 1-2: Trivial, always succeeds
   - 3-4: Easy, occasional failures
   - 5-6: Moderate challenge
   - 7-8: Difficult, requires good skill
   - 9-10: Expert level

3. **XP Guidelines**
   - Simple recipes: 5-15 XP
   - Moderate recipes: 15-30 XP
   - Complex recipes: 30-50 XP
   - Master recipes: 50-100 XP

4. **Auto-Learn**
   - Set `autolearn on` for common recipes
   - Set `autolearn off` for rare/special recipes that need a trainer or book

5. **Output Item**
   - Ensure the output item prototype exists
   - Recipe will fail if output item doesn't exist

## Troubleshooting

### "Recipe not found"
- Check vnum spelling
- Verify recipe was created with `recedit create`

### "Missing ingredient"
- Player needs exact ingredient or matching category
- For liquids, needs enough units in a container

### "Missing tool"
- Check tool location (inv/room/either)
- Verify tool exists and has correct category

### "Output item not found"
- Create the output item prototype first
- Verify output vnum is correct
