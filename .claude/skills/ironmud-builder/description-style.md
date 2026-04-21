# MUD Description Style Guide

This guide covers best practices for writing evocative, immersive descriptions for IronMUD entities.

## General Principles

### Length Guidelines

| Entity Type | Description Type | Target Length |
|-------------|-----------------|---------------|
| Room | description | 2-4 sentences |
| Item | short_desc | 5-15 words |
| Item | long_desc | 1-2 sentences |
| Mobile | short_desc | 5-15 words |
| Mobile | long_desc | 1-2 sentences |

### Perspective

- **Rooms**: Second person ("You stand in a dimly lit cavern...")
- **Items**: Third person ("A rusty sword lies here...")
- **Mobiles**: Third person ("A grizzled warrior stands here...")

### Tense

Always use present tense:
- "The wind howls through the passage." (not "The wind howled...")
- "Water drips from the ceiling." (not "Water was dripping...")

## Room Descriptions

### Structure

1. **Opening impression** - What the player first notices
2. **Spatial details** - Size, shape, notable features
3. **Atmospheric elements** - Sounds, smells, lighting
4. **Interactive hints** - Objects worth examining (without listing exits)

### Example

```
You stand at the entrance to an ancient crypt. Cold air rises from the
darkness below, carrying the musty scent of ages past. Worn stone steps
descend into shadow, their edges smoothed by countless footsteps. Faded
carvings on the walls hint at rituals long forgotten.
```

### Do's and Don'ts

**Do:**
- Use multiple senses (sight, sound, smell, temperature)
- Hint at history and atmosphere
- Create a sense of place
- Reference interactive elements naturally

**Don't:**
- List exits ("Exits: north, south") - the game handles this
- Use meta-game references ("This looks like a good place to rest")
- Break the fourth wall ("You have entered room 42")
- Use excessive adjectives ("The very extremely incredibly dark room")

## Item Descriptions

### Short Description (short_desc)

This appears in inventory and when examining. Should be a noun phrase, not a sentence.

**Good examples:**
- "a rusty iron sword with a notched blade"
- "a tattered leather satchel"
- "a glowing crystal pendant"

**Bad examples:**
- "This is a sword." (too generic)
- "The most amazing sword ever!" (too hyperbolic)
- "Sword" (too brief)

### Long Description (long_desc)

This appears when the item is on the ground. Should be a complete sentence ending with a period.

**Good examples:**
- "A rusty iron sword lies here, its notched blade catching the dim light."
- "A tattered leather satchel has been discarded in the corner."
- "A crystal pendant pulses with soft inner light, suspended in mid-air."

### Type-Specific Tips

| Item Type | Focus On |
|-----------|----------|
| Weapon | Balance, materials, lethal features, craftsmanship |
| Armor | Protection level, materials, how it would feel to wear |
| Container | Capacity hints, material, opening mechanism |
| Food | Appearance, aroma, freshness |
| Key | Unique identifying features, shape, material, engravings |

### Flag-Based Elements

When items have special flags, incorporate these naturally:

| Flag | Description Element |
|------|---------------------|
| glow | "glows softly", "emanates light" |
| hum | "hums with power", "vibrates faintly" |
| invisible | "shimmers", "flickers in and out" |
| unique | "radiates singular power", "feels one-of-a-kind" |
| boat | "sturdy hull", "floats on the surface", "waterproof construction" |

## Mobile Descriptions

### Short Description (short_desc)

Used in combat and when examining. Should describe the mobile as it would appear to a quick glance.

**Good examples:**
- "a grizzled old soldier with a scarred face"
- "a massive cave troll with mottled gray skin"
- "a kindly old herbalist in worn robes"

### Long Description (long_desc)

Appears when the mobile is in a room. Should be a complete sentence.

**Good examples:**
- "A grizzled old soldier stands here, eyeing you warily with his one good eye."
- "A massive cave troll squats in the corner, gnawing on a bone."
- "A kindly old herbalist sorts through bundles of dried plants."

### Role-Based Elements

| Role | Description Focus |
|------|-------------------|
| Aggressive monster | Threatening posture, predatory features, readiness to attack |
| Shopkeeper | Mercantile appearance, wares visible, welcoming demeanor |
| Healer | Healing imagery, herbs, kindness, wisdom |
| Guard | Watchfulness, weapons ready, alert stance |
| Trainer | Expertise, confidence, battle scars |

### Behavior Integration

Match the description to the mobile's behavior flags:

| Flag | Description Style |
|------|-------------------|
| aggressive | Hostile, threatening, ready to pounce |
| sentinel | Still, watchful, rooted in place |
| scavenger | Opportunistic, hungry, cunning |

## Atmospheric Themes

### Theme-Specific Elements

When building in a themed area, incorporate appropriate elements:

| Theme | Sensory Elements |
|-------|-----------------|
| Undead | Cold, decay, musty, bones, silence, dread |
| Forest | Trees, leaves, wildlife, dappled light, moss, birdsong |
| Cave | Stone, dripping water, echoes, stalactites, dampness |
| Castle | Stone walls, tapestries, torches, grandeur, architecture |
| Swamp | Murky water, mud, insects, decay, humidity, reeds |
| Desert | Sand, heat, dryness, wind, sun, mirages |
| Ocean | Salt, waves, wind, gulls, spray, endless horizon |
| Mountain | Peaks, thin air, rock, cold wind, vastness |
| City | Crowds, buildings, commerce, noise, streets |
| Dungeon | Stone, chains, darkness, damp, echoes, despair |
| Underwater | Murky depths, currents, bubbles, pressure, filtered light, silence |
| Coastal | Sand, tide pools, crashing waves, salt air, seaweed, driftwood |

## Using Context Tools

The MCP server provides tools to gather context before writing descriptions:

### get_room_context

Call this before writing room descriptions. Returns:
- Current room state and flags
- Area theme and level range
- Connected rooms (to hint at naturally)
- Suggested atmospheric elements

### get_item_context

Call this before writing item descriptions. Returns:
- Item properties (type, stats, flags)
- Type-specific writing guidance
- Flag-based elements to incorporate

### get_mobile_context

Call this before writing mobile descriptions. Returns:
- Mobile properties and role
- Behavior-based hints
- Area context for theming

### get_description_examples

Find existing descriptions for reference:
- Filter by area prefix for consistent style
- Filter by entity type or flags
- Useful for matching established patterns

## Example Workflow

1. **Create the entity** with placeholder description
2. **Call context tool** to gather information
3. **Review suggested elements** and theme
4. **Write description** incorporating context
5. **Update the entity** with new description

```
# Example: Creating a crypt room

1. create_room with title "Crypt Entrance", description "placeholder"
2. get_room_context("crypt:entrance", style_hints="atmospheric")
   -> Returns: theme="undead", flags={dark: true},
      suggested_elements=["cold", "decay", "darkness"]
3. Write: "Cold air rises from the darkness below, carrying the
   musty scent of ages past. Worn stone steps descend into shadow."
4. update_room with new description
```
