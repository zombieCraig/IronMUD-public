# Property Editor (pedit)

The property system allows builders to create rental property templates that players can lease. Each template can be instantiated multiple times, giving each tenant their own private copy.

## Core Concepts

### Templates vs Instances
- **Template** - The master design created by builders
- **Instance** - A copy created when a player leases the property

### Amenities
Items placed in template rooms with the `no_get` flag become amenities (furniture, fixtures) that appear in every instance but can't be taken.

### Entrance Room
One room in the template is marked as the entrance. This is where the portal to the property appears for tenants.

## Commands

### Creating Templates
```
pedit create <vnum>    - Create new property template
pedit edit <vnum>      - Enter existing template for editing
pedit done             - Exit template and return to game world
```

### Template Properties (while inside template)
```
pedit                  - Show template info
pedit name <text>      - Set display name
pedit desc <text>      - Set description
pedit rent <amount>    - Set monthly rent (gold)
pedit max <count>      - Set max instances (0=unlimited)
pedit level <num>      - Set level requirement (0=none)
pedit entrance         - Mark current room as entrance
```

## Building Workflow

### Step 1: Create the Template
```
pedit create apartment:studio
```
You're automatically moved to the template's entrance room.

### Step 2: Design the Layout
Use standard room commands while inside the template:
```
redit title Cozy Studio Apartment
redit desc A small but comfortable living space with exposed brick walls...
dig north kitchen
north
redit title Small Kitchen
redit desc A compact kitchen with just enough room for cooking...
south
```

### Step 3: Add Amenities (Furniture)
Create items with `no_get` flag and place them:
```
oedit create apartment:bed
oedit apartment:bed name "a comfortable bed"
oedit apartment:bed short "A comfortable bed sits against the wall."
oedit apartment:bed flags no_get
get bed              # Get one from void (if prototype)
drop bed             # Place in room
```

### Step 4: Configure Template Settings
```
pedit name "Studio Apartment"
pedit desc "A cozy studio apartment perfect for a single adventurer."
pedit rent 50
pedit level 5
pedit max 10
```

### Step 5: Mark Entrance and Finish
```
# Go to the room you want as the entrance
pedit entrance
pedit done
```

## Example: Complete Apartment Template

```
# Create the template
pedit create apt:luxury

# Design entrance/living room
redit title Luxury Apartment - Living Room
redit desc Sunlight streams through floor-to-ceiling windows...
redit flags indoors

# Add living room furniture
oedit create apt:sofa
oedit apt:sofa name "a plush leather sofa"
oedit apt:sofa flags no_get
drop sofa

# Create bedroom
dig north bedroom
north
redit title Luxury Apartment - Bedroom
redit desc A spacious bedroom with a king-sized bed...
redit flags indoors

# Add bedroom furniture
oedit create apt:kingbed
oedit apt:kingbed name "a king-sized bed"
oedit apt:kingbed flags no_get
drop kingbed

# Create bathroom
dig east bathroom
east
redit title Luxury Apartment - Bathroom
redit desc Marble tiles and gold fixtures adorn this bathroom...
redit flags indoors

# Return to entrance and configure
south
pedit entrance
pedit name "Luxury Apartment"
pedit desc "A prestigious apartment with stunning city views."
pedit rent 500
pedit level 15
pedit max 5

# Finish
pedit done
```

## Best Practices

1. **Naming Convention**
   - Use consistent vnum prefix (e.g., `apt:`, `house:`, `shop:`)
   - Keep template vnums descriptive (`apt:studio`, `apt:2br`, `house:cottage`)

2. **Room Design**
   - Always set `indoors` flag
   - Write atmospheric descriptions
   - Keep layouts intuitive

3. **Amenities**
   - Always use `no_get` flag for furniture
   - Add varied furniture types for atmosphere
   - Consider functional items (beds for resting)

4. **Pricing Guidelines**
   - Simple studio: 25-75 gold/month
   - Standard apartment: 75-200 gold/month
   - Luxury apartment: 200-500 gold/month
   - House: 500-2000 gold/month

5. **Level Requirements**
   - Low-level (1-10): No requirement or level 1-5
   - Mid-level (10-20): Level 10-15
   - High-level (20+): Level 15-25

## Troubleshooting

### "Template has no entrance room"
- Enter the template with `pedit edit <vnum>`
- Go to desired entrance room
- Run `pedit entrance`
- Run `pedit done`

### "Furniture disappears"
- Ensure items have `no_get` flag set
- Items without `no_get` can be picked up by players

### "Can't find template"
- Check vnum spelling
- Use exact vnum including prefix
