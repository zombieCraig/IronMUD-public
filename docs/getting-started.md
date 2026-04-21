# Getting Started Guide

This guide walks you through your first steps with IronMUD, from connecting to exploring the demo world to building your own content.

## Quick Start

1. **Install and run** (see [Installation](installation.md) for details):
   ```bash
   cargo build --release
   cargo run --release --bin ironmud
   ```

2. **Connect** with any MUD client or telnet:
   ```bash
   telnet localhost 4000
   ```

3. **Create your character**. The first character on a fresh database automatically becomes an administrator with full builder permissions.

4. **Look around**. You start in Oakvale Village's Town Square, the heart of the demo world.

## Exploring the Demo World

On first startup, IronMUD seeds a medieval fantasy demo world with approximately 55 rooms across 5 themed areas. This world showcases the engine's major features.

### World Map

```
                    Whispering Woods (level 2-6)
                         |
                    North Gate
                         |
    Iron Keep ── East Road ── TOWN SQUARE ── Market Street
    (level 3-8)              (Oakvale Hub)
         |                       |
    Shadowfang Caves        South Path
    (level 5-10)                 |
                          Hilltop Farm (level 1-3)
```

### Oakvale Village (Hub, 15 rooms)

The central hub where players begin. Contains essential services:

| Location | What's Here |
|----------|-------------|
| Town Square | Starting room, fountain, village guard |
| The Rusty Tankard | Tavern with Old Torvald (food & drink shop) |
| General Store | Elara the Merchant (supplies, tools) |
| The Iron Anvil | Grimjaw the Blacksmith (weapons) |
| Temple of Light | Sister Maren (healer), free healing potion |
| Oakvale Bank | Aldwin the Banker (deposit/withdraw gold) |
| Post Office | Pip the Postmaster (mail system) |
| Village Garden | Gardening plots for planting |
| Cottage Lane | Fenwick the Estate Agent (rent a home) |
| Market Street | City foraging area |

### Whispering Woods (Wilderness, 11 rooms)

An enchanted forest with paths, clearings, and dangers:
- **Fishing Pond** - Freshwater fishing (trout, bass)
- **Herb Garden** - Herb foraging
- **Wolf Den** - Grey wolves (level 2, aggressive but cowardly)
- **Dark Hollow** - Passage to the Shadowfang Caves

### Iron Keep (Castle, 12 rooms)

A stone fortress with shops and a tower:
- **Gatehouse** - Locked iron gate (key held by the knight)
- **Armory** - Ser Aldric (armor shop)
- **Great Hall** - Crafting area
- **Kitchen** - Cooking area
- **Tower** - Elevator transport with 3 stops (base, gallery, rooftop)

### Shadowfang Caves (Dungeon, 10 rooms)

Dark caves beneath Iron Keep with aggressive monsters:
- **Goblin Camp** - Goblin raiders (level 3)
- **Spider Nest** - Cave spider (level 4, poison)
- **Drake Lair** - Shadow Drake boss (level 8, 30-min respawn, 25% chance to drop Shadow Blade)
- **Treasure Alcove** - Locked with a key dropped by the drake
- **Underground Pool** - Magical fishing (crystal fish)
- **Fungal Grotto** - Glowing mushroom foraging

### Hilltop Farm (Crafting, 8 rooms)

A pastoral farm for crafting and gardening:
- **Farmhouse** - Cooking area, Old Barley the Farmer
- **Workshop** - Crafting area
- **Garden Plots** - Planting area with dirt floor
- **Orchard** - Apple foraging
- **Wheat Field** - Seasonal descriptions

## Feature Showcase Checklist

The demo world demonstrates these engine features. Try each one:

- [ ] **Shops** - `list` and `buy` at any shopkeeper
- [ ] **Healing** - `heal` at the temple priestess
- [ ] **Combat** - Fight wolves in the woods or goblins in the caves
- [ ] **Fishing** - `fish` at the pond or underground pool
- [ ] **Foraging** - `forage` in wilderness or city areas
- [ ] **Cooking** - `cook` in the kitchen (need ingredients)
- [ ] **Crafting** - `craft` in the workshop or great hall
- [ ] **Gardening** - `plant` seeds in garden plots, `water` them, `harvest` later
- [ ] **Doors** - `unlock`/`open` the Iron Keep gate (get key from knight)
- [ ] **Transport** - Ride the tower elevator (`call`, `push` buttons)
- [ ] **Properties** - `lease list` at the estate agent, rent a cottage
- [ ] **Mail** - `mail send <player>` at the post office
- [ ] **Banking** - `deposit`/`withdraw` gold at the bank
- [ ] **NPC Dialogue** - `say hello` near any NPC with dialogue
- [ ] **Daily Routines** - Watch NPCs move between locations throughout the day
- [ ] **Boss Fight** - Defeat the Shadow Drake for rare loot

## Choosing a Theme

IronMUD ships with two theme presets for classes and races:

| Preset | Classes | Races |
|--------|---------|-------|
| `fantasy` (default) | Peasant, Warrior, Mage, Ranger, Cleric, Rogue, Bard, Alchemist | Human, Elf, Dwarf, Halfling, Orc, Gnome, Half-Elf, Dragonborn |
| `modern` | Civilian, Soldier, Medic, Engineer, Mechanic, etc. | Human (and variants) |

### Switching Presets

```bash
# Switch to modern theme
ironmud-admin settings set class_preset modern
ironmud-admin settings set race_preset modern

# Switch back to fantasy
ironmud-admin settings set class_preset fantasy
ironmud-admin settings set race_preset fantasy
```

Restart the server after changing presets. Existing characters keep their current class/race.

### Customizing Presets

Class definitions live in `scripts/data/classes_<preset>.json`. Copy an existing file and modify it:

```bash
cp scripts/data/classes_fantasy.json scripts/data/classes_scifi.json
# Edit the new file, then:
ironmud-admin settings set class_preset scifi
```

Race suggestions are in `scripts/data/race_suggestions_<preset>.json` (same pattern).

## Building Your First Area

Once you're comfortable with the demo world, try creating your own area. All building is done in-game using Online Creation (OLC) editors.

### Step 1: Create an Area

```
aedit create myarea "My First Area"
aedit desc A testing ground for my first area.
aedit theme fantasy dungeon
aedit levels 1 5
aedit done
```

### Step 2: Create Rooms

```
redit create myarea:entrance "Dungeon Entrance"
redit desc You stand at the mouth of a dark cave. Cold air flows outward.
redit flag indoors on
redit done
```

Connect rooms with exits:
```
redit myarea:entrance
redit exit north myarea:hallway
redit done
```

### Step 3: Create Items

```
oedit create myarea:rusty_key "a rusty key"
oedit type key
oedit short A rusty iron key lies on the ground.
oedit long An old key, pitted with rust but still functional.
oedit done
```

### Step 4: Create NPCs

```
medit create myarea:guard "a dungeon guard"
medit short A dungeon guard blocks the way.
medit long A hulking guard in dented armor watches the corridor.
medit level 3
medit flag aggressive on
medit done
```

### Step 5: Set Up Spawn Points

```
spedit create myarea:guard myarea:entrance mobile
spedit max 1
spedit interval 300
spedit done
```

For complete editor documentation, see the [Builder Guide](builder-guide.md).

## Server Configuration

Key settings you may want to configure:

| Setting | Default | Description |
|---------|---------|-------------|
| `builder_mode` | `all` | Who can toggle builder status (`all`, `granted`, `none`) |
| `class_preset` | `fantasy` | Active class preset |
| `race_preset` | `fantasy` | Active race preset |
| `starting_room_id` | (seeded town square) | Room vnum (e.g. `oakvale:square`) where new characters spawn. Falls back to the default town square if unset or the vnum doesn't resolve. |

Manage settings with:
```bash
ironmud-admin settings list
ironmud-admin settings set <key> <value>
```

## Managing Players

```bash
# List all users with their permissions
ironmud-admin user list

# Grant admin access
ironmud-admin user grant-admin <charactername>

# Grant builder access
ironmud-admin user grant-builder <charactername>

# Force password change on next login
ironmud-admin user require-password-change <charactername>
```

## World Management

### Getting World Stats

```bash
ironmud-admin world info
```

Shows counts of all entity types (areas, rooms, items, mobiles, spawn points, etc.).

### Starting Fresh

If you want to wipe the demo world and start from scratch:

```bash
# Back up first!
cp -r ironmud.db ironmud.db.backup

# Clear all world data (keeps character accounts and settings)
ironmud-admin world clear
```

Type `CONFIRM` when prompted. On next server start, the demo world will re-seed automatically. To prevent re-seeding, create at least one area of your own before clearing.

## Backup and Maintenance

### Database Backup

```bash
# Stop the server first for a clean backup
sudo systemctl stop ironmud
cp -r /opt/ironmud/ironmud.db /backup/ironmud.db-$(date +%Y%m%d)
sudo systemctl start ironmud
```

### Upgrading

```bash
cd /path/to/IronMUD
git pull
sudo ./install.sh    # For systemd installs
# OR
cargo build --release  # For manual installs
```

The install script handles backup, rebuild, and restart automatically.

## Common Gotchas

| Issue | Cause | Solution |
|-------|-------|----------|
| "Database locked" error | Another server instance is running | Stop the other instance first |
| Scripts not updating | Script cache | Use `reload` in-game to hot-reload scripts |
| NPC not appearing | Missing spawn point | Create a spawn point with `spedit` |
| Items not respawning | No item spawn point | Items only respawn if they have spawn points |
| Elevator not working | Transport not connected | Use `call` to summon it, `push` to travel |

## Next Steps

- [Player Guide](player-guide.md) - Full command reference
- [Builder Guide](builder-guide.md) - Complete OLC documentation
- [Admin Guide](admin-guide.md) - Server administration, Matrix/Discord/AI integration
- [Installation](installation.md) - Production deployment
