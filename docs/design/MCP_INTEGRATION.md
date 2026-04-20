# Claude Code MCP Integration Design

This document describes the design for integrating Claude Code with IronMUD to enable AI-assisted area building.

## Overview

The integration allows Claude Code to create and modify MUD content (areas, rooms, items, mobiles, spawn points) through a Model Context Protocol (MCP) server that communicates with IronMUD via a REST API.

## Architecture

```
┌─────────────┐     ┌─────────────────┐     ┌─────────────────┐     ┌──────────────┐
│ Claude Code │────►│  MCP Server     │────►│  REST API       │────►│ Sled DB      │
│             │     │  (TypeScript)   │     │  (Rust/Axum)    │     │              │
└─────────────┘     └─────────────────┘     └────────┬────────┘     └──────────────┘
                                                     │
                                                     ▼
                                            ┌─────────────────┐
                                            │ SharedConnections│
                                            │ (notify players) │
                                            └─────────────────┘
```

### Components

1. **REST API** (Rust/Axum, port 4001)
   - CRUD endpoints for all building entities
   - API key authentication
   - Area permission enforcement
   - Builder notifications

2. **MCP Server** (TypeScript/Node.js)
   - Implements Model Context Protocol
   - Exposes building tools to Claude Code
   - Translates tool calls to REST API requests

3. **Admin CLI**
   - API key management commands
   - Key generation, listing, revocation

## REST API Specification

### Authentication

All requests require Bearer token authentication:

```
Authorization: Bearer <api_key>
```

API keys are stored with Argon2 hashes in the database and linked to a character name for permission checking.

### Base URL

```
http://localhost:4001/api/v1
```

### Endpoints

#### Health Check
```
GET /health
Response: { "status": "ok", "version": "0.1.0" }
```

#### Areas

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/areas` | List all areas |
| GET | `/areas/{id}` | Get area by UUID |
| GET | `/areas/by-prefix/{prefix}` | Get area by prefix |
| POST | `/areas` | Create new area |
| PUT | `/areas/{id}` | Update area |
| DELETE | `/areas/{id}` | Delete area (rooms unassigned) |
| POST | `/areas/{id}/reset` | Trigger area respawn |
| GET | `/areas/{id}/rooms` | List rooms in area |

**Create Area Request:**
```json
{
    "name": "Dark Forest",
    "prefix": "forest",
    "description": "A mysterious forest shrouded in eternal twilight.",
    "level_min": 5,
    "level_max": 15,
    "theme": "nature"
}
```

#### Rooms

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/rooms` | List rooms (paginated) |
| GET | `/rooms/{id}` | Get room by UUID |
| GET | `/rooms/by-vnum/{vnum}` | Get room by vnum |
| POST | `/rooms` | Create new room |
| PUT | `/rooms/{id}` | Update room |
| DELETE | `/rooms/{id}` | Delete room |
| PUT | `/rooms/{id}/exits/{direction}` | Set exit |
| DELETE | `/rooms/{id}/exits/{direction}` | Remove exit |
| PUT | `/rooms/{id}/doors/{direction}` | Add/update door |
| DELETE | `/rooms/{id}/doors/{direction}` | Remove door |
| POST | `/rooms/{id}/triggers` | Add trigger |
| DELETE | `/rooms/{id}/triggers/{index}` | Remove trigger |
| POST | `/rooms/{id}/extra` | Add extra description |
| DELETE | `/rooms/{id}/extra/{keyword}` | Remove extra description |

**Create Room Request:**
```json
{
    "title": "Forest Entrance",
    "description": "Tall trees tower above you, their branches forming a canopy that blocks most of the sunlight.",
    "area_id": "uuid-here",
    "vnum": "forest:entrance",
    "flags": {
        "dark": false,
        "safe": false,
        "no_mob": false,
        "indoors": false
    }
}
```

**Set Exit Request:**
```json
{
    "target_room_id": "uuid-of-target-room"
}
```

**Add Door Request:**
```json
{
    "name": "wooden door",
    "description": "A sturdy wooden door blocks the way.",
    "is_closed": true,
    "is_locked": false,
    "key_vnum": "forest:old_key"
}
```

#### Items

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/items` | List items (paginated) |
| GET | `/items/prototypes` | List prototype items only |
| GET | `/items/{id}` | Get item by UUID |
| GET | `/items/by-vnum/{vnum}` | Get item by vnum |
| POST | `/items` | Create item prototype |
| PUT | `/items/{id}` | Update item |
| DELETE | `/items/{id}` | Delete item |
| POST | `/items/{vnum}/spawn` | Spawn item from prototype |

**Create Item Request:**
```json
{
    "name": "Iron Sword",
    "short_desc": "An iron sword lies here.",
    "long_desc": "This is a well-crafted iron sword with a leather-wrapped handle.",
    "vnum": "weapons:iron_sword",
    "keywords": ["sword", "iron", "weapon"],
    "item_type": "weapon",
    "weight": 5,
    "value": 100,
    "wear_locations": ["wield"],
    "damage_dice_count": 2,
    "damage_dice_sides": 6,
    "damage_type": "slash",
    "flags": {
        "no_drop": false,
        "magic": false
    }
}
```

#### Mobiles

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/mobiles` | List mobiles (paginated) |
| GET | `/mobiles/prototypes` | List prototype mobiles only |
| GET | `/mobiles/{id}` | Get mobile by UUID |
| GET | `/mobiles/by-vnum/{vnum}` | Get mobile by vnum |
| POST | `/mobiles` | Create mobile prototype |
| PUT | `/mobiles/{id}` | Update mobile |
| DELETE | `/mobiles/{id}` | Delete mobile |
| POST | `/mobiles/{id}/dialogue` | Add dialogue entry |
| DELETE | `/mobiles/{id}/dialogue/{keyword}` | Remove dialogue entry |
| POST | `/mobiles/{vnum}/spawn` | Spawn mobile from prototype |

**Create Mobile Request:**
```json
{
    "name": "Forest Wolf",
    "short_desc": "A grey wolf prowls here, watching you warily.",
    "long_desc": "This wolf has thick grey fur and sharp yellow eyes.",
    "vnum": "forest:wolf",
    "keywords": ["wolf", "grey", "animal"],
    "level": 8,
    "max_hp": 45,
    "damage_dice": "2d4+2",
    "armor_class": 12,
    "flags": {
        "aggressive": true,
        "sentinel": false,
        "scavenger": false
    }
}
```

**Add Dialogue Request:**
```json
{
    "keyword": "hello",
    "response": "The old man nods slowly. \"Greetings, traveler.\""
}
```

#### Spawn Points

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/spawn-points` | List all spawn points |
| GET | `/spawn-points/{id}` | Get spawn point by UUID |
| POST | `/spawn-points` | Create spawn point |
| PUT | `/spawn-points/{id}` | Update spawn point |
| DELETE | `/spawn-points/{id}` | Delete spawn point |
| POST | `/spawn-points/{id}/dependencies` | Add dependency |
| DELETE | `/spawn-points/{id}/dependencies/{index}` | Remove dependency |

**Create Spawn Point Request:**
```json
{
    "area_id": "uuid-here",
    "room_id": "uuid-here",
    "entity_type": "mobile",
    "vnum": "forest:wolf",
    "max_count": 3,
    "respawn_interval_secs": 300,
    "enabled": true
}
```

**Add Dependency Request:**
```json
{
    "item_vnum": "forest:wolf_pelt",
    "destination": "inventory",
    "count": 1
}
```

#### Search & Utilities

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/search?q={query}&type={type}` | Search entities |
| POST | `/vnums/generate` | Generate unique vnum |

**Generate Vnum Request:**
```json
{
    "entity_type": "room",
    "prefix": "forest",
    "base_name": "clearing"
}
```

**Generate Vnum Response:**
```json
{
    "vnum": "forest:clearing_1"
}
```

### Error Responses

All errors return JSON with consistent format:

```json
{
    "success": false,
    "error": {
        "code": "VNUM_IN_USE",
        "message": "The vnum 'forest:entrance' is already assigned to another room"
    }
}
```

Error codes:
- `UNAUTHORIZED` - Missing or invalid API key
- `FORBIDDEN` - Insufficient permissions
- `NOT_FOUND` - Resource not found
- `VNUM_IN_USE` - Vnum already exists
- `INVALID_INPUT` - Validation failed
- `CONFLICT` - Resource conflict
- `INTERNAL_ERROR` - Server error

## MCP Server Specification

### Tools

The MCP server exposes the following tools to Claude Code:

#### Area Tools
- `list_areas()` - List all areas
- `get_area(identifier)` - Get area by UUID or prefix
- `create_area(name, prefix, description?, level_min?, level_max?, theme?)` - Create area
- `update_area(id, ...)` - Update area properties
- `delete_area(id)` - Delete area
- `reset_area(id)` - Trigger respawn

#### Room Tools
- `list_rooms_in_area(area_id)` - List rooms in area
- `get_room(identifier)` - Get room by UUID or vnum
- `create_room(title, description, area_id?, vnum?, flags?)` - Create room
- `update_room(id, ...)` - Update room
- `delete_room(id)` - Delete room
- `set_room_exit(room_id, direction, target_room_id)` - Connect rooms
- `remove_room_exit(room_id, direction)` - Disconnect rooms
- `add_room_door(room_id, direction, name, is_closed?, is_locked?, key_vnum?, description?)` - Add door
- `add_room_trigger(room_id, trigger_type, script_name, args?, interval?, chance?)` - Add trigger
- `add_room_extra_desc(room_id, keywords, description)` - Add extra description
- `search_rooms(query)` - Search by keyword

#### Item Tools
- `list_items(item_type?, prototypes_only?)` - List items
- `get_item(identifier)` - Get item by UUID or vnum
- `create_item(...)` - Create item prototype
- `update_item(id, ...)` - Update item
- `delete_item(id)` - Delete item
- `search_items(query)` - Search by keyword

#### Mobile Tools
- `list_mobiles(prototypes_only?)` - List mobiles
- `get_mobile(identifier)` - Get mobile by UUID or vnum
- `create_mobile(...)` - Create mobile prototype
- `update_mobile(id, ...)` - Update mobile
- `delete_mobile(id)` - Delete mobile
- `add_mobile_dialogue(mobile_id, keyword, response)` - Add dialogue
- `search_mobiles(query)` - Search by keyword

#### Spawn Point Tools
- `list_spawn_points(area_id?)` - List spawn points
- `create_spawn_point(area_id, room_id, entity_type, vnum, max_count?, respawn_interval_secs?)` - Create
- `update_spawn_point(id, ...)` - Update
- `delete_spawn_point(id)` - Delete
- `add_spawn_dependency(spawn_point_id, item_vnum, destination, wear_location?, count?)` - Add dependency

### Resources

The MCP server provides these read-only resources:

- `ironmud://areas` - JSON list of all areas
- `ironmud://area/{prefix}` - Area data by prefix
- `ironmud://room/{vnum}` - Room data by vnum
- `ironmud://item/{vnum}` - Item data by vnum
- `ironmud://mobile/{vnum}` - Mobile data by vnum

### Configuration

Environment variables:
- `IRONMUD_API_URL` - REST API base URL (default: `http://localhost:4001`)
- `IRONMUD_API_KEY` - API key for authentication

## Data Structures

### ApiKey

```rust
pub struct ApiKey {
    pub id: Uuid,
    pub key_hash: String,           // Argon2 hash of the key
    pub name: String,               // Human-readable name
    pub owner_character: String,    // Character name for permission checks
    pub permissions: ApiPermissions,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub enabled: bool,
}

pub struct ApiPermissions {
    pub read: bool,     // Can read data
    pub write: bool,    // Can modify data
    pub admin: bool,    // Bypass area permission checks
}
```

## Permission Model

API requests follow IronMUD's existing area permission system:

1. API key must be enabled and have appropriate permissions (read/write)
2. API key is linked to a character name
3. For write operations on area-owned entities:
   - Admin API keys bypass permission checks
   - Otherwise, check if character is area owner or trusted builder
   - `AreaPermission::OwnerOnly` - Only owner can edit
   - `AreaPermission::Trusted` - Owner and trusted builders can edit
   - `AreaPermission::AllBuilders` - Any builder can edit

## Builder Notifications

When the API modifies content, connected builders receive notifications:

```
[API] Room 'forest:entrance' created by claude-code-key
[API] Area 'Dark Forest' reset: 12 entities spawned
```

Players in affected rooms receive appropriate messages:

```
The room shimmers momentarily as reality shifts.
```

## Admin CLI Commands

```bash
# Generate new API key
ironmud-admin api-key create --name "claude-code" --character "craig" --write

# List all API keys
ironmud-admin api-key list

# Revoke an API key
ironmud-admin api-key revoke <key-id>

# Show API key details (without revealing the key)
ironmud-admin api-key show <key-id>
```

## Security Considerations

1. **API Key Storage** - Keys are hashed with Argon2 before storage
2. **Transport Security** - Use HTTPS via reverse proxy in production
3. **Rate Limiting** - Implement via tower-http middleware
4. **Input Validation** - Validate all inputs before database operations
5. **Permission Enforcement** - All write operations check area permissions

## Implementation Files

### Rust (REST API)

| File | Purpose |
|------|---------|
| `src/api/mod.rs` | Module root, router, state, error types |
| `src/api/auth.rs` | API key validation middleware |
| `src/api/areas.rs` | Area CRUD handlers |
| `src/api/rooms.rs` | Room CRUD handlers |
| `src/api/items.rs` | Item CRUD handlers |
| `src/api/mobiles.rs` | Mobile CRUD handlers |
| `src/api/spawn.rs` | Spawn point handlers |
| `src/lib.rs` | ApiKey struct definition |
| `src/db.rs` | API key database operations |
| `src/main.rs` | Spawn API server |
| `src/bin/ironmud-admin.rs` | Admin CLI commands |

### TypeScript (MCP Server)

| File | Purpose |
|------|---------|
| `src/index.ts` | MCP server entry point |
| `src/api-client.ts` | HTTP client for REST API |
| `src/tools/areas.ts` | Area tool handlers |
| `src/tools/rooms.ts` | Room tool handlers |
| `src/tools/items.ts` | Item tool handlers |
| `src/tools/mobiles.ts` | Mobile tool handlers |
| `src/tools/spawn-points.ts` | Spawn point tool handlers |
| `src/types.ts` | TypeScript interfaces |

## Dependencies

### Rust (Cargo.toml additions)

```toml
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
```

### TypeScript (package.json)

```json
{
  "name": "ironmud-mcp",
  "version": "0.1.0",
  "type": "module",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0",
    "axios": "^1.6.0"
  },
  "devDependencies": {
    "typescript": "^5.0.0",
    "@types/node": "^20.0.0"
  }
}
```

## Claude Skill: IronMUD Builder

The MCP server provides *tools* (functions Claude can call), but Claude also needs *knowledge* about IronMUD's game mechanics to use those tools effectively. This is provided via a Claude Skill.

### What is a Claude Skill?

A Skill is a markdown file that teaches Claude domain-specific knowledge. It's automatically discovered and loaded when Claude determines it's relevant to the task.

### Skill Structure

```
.claude/skills/ironmud-builder/
├── SKILL.md              # Main skill file (auto-discovered)
├── mechanics.md          # Core game mechanics reference
├── entity-guide.md       # Rooms, items, mobiles, spawn points
├── building-patterns.md  # Common area design patterns
└── checklists.md         # Step-by-step building workflows
```

### SKILL.md Content

```yaml
---
name: ironmud-builder
description: Build MUD areas with rooms, items, mobiles, and spawn points. Use when creating game content, designing dungeons, or populating areas with NPCs and items.
---

# IronMUD Area Builder

You are helping build content for IronMUD, a text-based multiplayer game.

## Core Concepts

1. **Areas** group related rooms together (forest, castle, dungeon)
2. **Rooms** are connected by exits (north/south/east/west/up/down)
3. **Items** are prototypes that spawn instances (sword template → actual sword)
4. **Mobiles** are NPCs that can be aggressive, shopkeepers, or passive
5. **Spawn Points** control respawning of mobiles/items after death/pickup

## Critical Dependencies

When building an area, you MUST follow this order:
1. Create the area (establishes prefix for vnums)
2. Create room prototypes (use area prefix: "forest:entrance")
3. Connect rooms with exits
4. Create item prototypes (weapons, keys, loot)
5. Create mobile prototypes (NPCs, monsters)
6. Create spawn points to make mobiles/items appear
7. Add spawn dependencies (equipment that spawns with mobiles)

**WARNING**: Mobiles and items without spawn points will NOT respawn after death/pickup!
```

### Why the Skill is Essential

Without the Skill, Claude might create beautiful rooms and NPCs but forget spawn points—meaning after a player kills a monster or picks up an item, it would never respawn. The Skill teaches Claude:

- **Mechanics**: Room flags, mobile flags, item types, trigger types
- **Patterns**: Boss lair design, road encounters, locked doors
- **Checklists**: Ensuring all dependencies are met

### MCP + Skill Interaction

| Component | Provides | Example |
|-----------|----------|---------|
| MCP Server | Tools (functions) | `create_room()`, `create_spawn_point()` |
| Skill | Knowledge (instructions) | "Mobiles need spawn points to respawn" |
| Together | Intelligent building | Create functional, respawning areas |

## Usage Example

Once implemented, Claude Code can be used like this:

**Simple Example:**
```
User: Create a small forest area with 5 rooms connected in a line

Claude Code: I'll create a forest area with 5 connected rooms.

[Uses create_area tool to create "Small Forest" with prefix "smallforest"]
[Uses create_room tool 5 times to create rooms]
[Uses set_room_exit tool to connect rooms north-south]

Done! Created area "Small Forest" with 5 rooms:
- smallforest:entrance (south end)
- smallforest:path_1
- smallforest:path_2
- smallforest:path_3
- smallforest:clearing (north end)

All rooms are connected north-south. Would you like me to add descriptions, items, or mobiles?
```

**Complex Example (with Skill knowledge):**
```
User: Create a dark forest path that leads to a vast castle. At the top of
one of the towers is the head vampire. The road also has bandits.

Claude Code: I'll create a complete area with the vampire's castle. Let me plan this out:

[Skill knowledge: This is a level progression - road encounters → boss lair pattern]

1. Creating area "Evil Forest" with prefix "evilforest"...
2. Creating 6 forest path rooms with `dark` flag...
3. Creating castle exterior (gatehouse, courtyard)...
4. Creating castle interior and tower rooms...
5. Connecting all rooms with exits...

Now adding NPCs:
6. Creating bandit mobile prototype (aggressive, level 5-8)...
7. Creating vampire mobile prototype (sentinel, level 20, high HP)...

[Skill knowledge: Mobiles need spawn points!]

8. Creating spawn points for bandits in forest rooms (respawn: 300s)...
9. Creating spawn point for vampire in tower top (respawn: 1800s)...
10. Adding spawn dependencies - vampire spawns with magic cloak equipped...

Done! Created 12 rooms, 2 mobile prototypes, and 4 spawn points.
The bandits patrol the forest path and will respawn after 5 minutes.
The vampire awaits in the tower and respawns after 30 minutes.
```

## Future Enhancements

- **WebSocket support** - Real-time updates when content changes
- **Bulk operations** - Create multiple rooms in one call
- **Templates** - Area templates with pre-configured rooms/items/mobiles
- **Change history** - Undo/redo via audit log
- **AI content generation** - Endpoints for Claude to generate descriptions
