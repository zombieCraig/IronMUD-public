# IronMUD MCP Server

Model Context Protocol (MCP) server for integrating Claude Code with IronMUD's building system.

## Overview

This MCP server allows Claude Code to create and modify MUD content (areas, rooms, items, mobiles, spawn points) through tools that communicate with IronMUD's REST API.

## Quick Start (Claude Code CLI)

### 1. Start IronMUD with REST API enabled

```bash
IRONMUD_API_ENABLED=true cargo run
```

### 2. Create an API key

Connect to the MUD as an admin character and run:
```
apikey create claude-code read write
```

Save the key that's displayed - you'll need it in step 4.

### 3. Build the MCP server

```bash
cd mcp-server
npm install
npm run build
```

### 4. Configure Claude Code

Add to your project settings (`.claude/settings.local.json`):

```json
{
  "mcpServers": {
    "ironmud": {
      "command": "node",
      "args": ["mcp-server/dist/index.js"],
      "env": {
        "IRONMUD_API_KEY": "your-api-key-here",
        "IRONMUD_API_URL": "http://localhost:4001/api/v1"
      }
    }
  }
}
```

Or add globally to `~/.claude/settings.json` (use absolute paths).

### 5. Install the builder skill (optional but recommended)

```bash
./scripts/install-claude-skill.sh
```

### 6. Restart Claude Code

The MCP server will connect automatically. Test with:
```
Use the /ironmud-builder skill to list all areas
```

### Connecting to a Different Server

Change `IRONMUD_API_URL` to point to your server:

```json
"env": {
  "IRONMUD_API_KEY": "your-key",
  "IRONMUD_API_URL": "http://your-server:4001/api/v1"
}
```

## Installation

```bash
cd mcp-server
npm install
npm run build
```

## Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `IRONMUD_API_URL` | REST API base URL | `http://localhost:4001/api/v1` |
| `IRONMUD_API_KEY` | API key for authentication | *Required* |

### Getting an API Key (Admin CLI)

If you have shell access to the server, you can also use the admin CLI:

```bash
ironmud-admin api-key create --name "claude-code" --character "yourname" --read --write
```

## Usage with Claude Desktop

Add to your Claude Desktop configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "ironmud": {
      "command": "node",
      "args": ["/path/to/mcp-server/dist/index.js"],
      "env": {
        "IRONMUD_API_KEY": "your-api-key-here",
        "IRONMUD_API_URL": "http://localhost:4001/api/v1"
      }
    }
  }
}
```

### Installing the Builder Skill

For best results, also install the IronMUD builder skill which provides Claude with knowledge about IronMUD's building system, entity relationships, and best practices:

```bash
./scripts/install-claude-skill.sh
```

The skill includes documentation for:
- Core building concepts (areas, rooms, items, mobiles, spawn points)
- Specialized editors (tedit for transports, pedit for properties, spedit for spawn points, recedit for recipes)
- Common building patterns and layouts
- Game mechanics reference
- Step-by-step checklists

## Available Tools

### Area Tools
- `list_areas` - List all areas
- `get_area` - Get area by UUID or prefix
- `create_area` - Create a new area
- `update_area` - Update area properties
- `delete_area` - Delete an area
- `reset_area` - Trigger area respawn
- `list_rooms_in_area` - List rooms in an area

### Room Tools
- `get_room` - Get room by UUID or vnum
- `create_room` - Create a new room
- `update_room` - Update room properties
- `delete_room` - Delete a room
- `set_room_exit` - Connect two rooms
- `remove_room_exit` - Remove an exit
- `add_room_door` - Add a door to an exit
- `remove_room_door` - Remove a door
- `add_room_trigger` - Add a script trigger
- `remove_room_trigger` - Remove a trigger
- `add_room_extra_desc` - Add examine description
- `remove_room_extra_desc` - Remove extra description

### Item Tools
- `list_items` - List items
- `list_item_prototypes` - List prototype items only
- `get_item` - Get item by UUID or vnum
- `create_item` - Create item prototype
- `update_item` - Update item
- `delete_item` - Delete item
- `spawn_item` - Spawn instance from prototype

### Mobile Tools
- `list_mobiles` - List mobiles
- `list_mobile_prototypes` - List prototype mobiles only
- `get_mobile` - Get mobile by UUID or vnum
- `create_mobile` - Create mobile prototype
- `update_mobile` - Update mobile
- `delete_mobile` - Delete mobile
- `add_mobile_dialogue` - Add dialogue entry
- `remove_mobile_dialogue` - Remove dialogue
- `spawn_mobile` - Spawn instance from prototype

### Spawn Point Tools
- `list_spawn_points` - List spawn points
- `get_spawn_point` - Get spawn point by UUID
- `create_spawn_point` - Create spawn point
- `update_spawn_point` - Update spawn point
- `delete_spawn_point` - Delete spawn point
- `add_spawn_dependency` - Add item to spawn with entity
- `remove_spawn_dependency` - Remove dependency

### Description Context Tools
These tools gather contextual information to help generate rich, thematic descriptions:

- `get_room_context` - Get area theme, connected rooms, and suggested atmospheric elements
- `get_item_context` - Get type-specific guidance and flag-based elements (glow, hum, etc.)
- `get_mobile_context` - Get role detection (merchant, guard, monster) and behavior hints
- `get_description_examples` - Find example descriptions from existing entities

## Resources

The server also exposes read-only resources:

- `ironmud://areas` - JSON list of all areas
- `ironmud://area/{prefix}` - Area data by prefix
- `ironmud://room/{vnum}` - Room data by vnum
- `ironmud://item/{vnum}` - Item data by vnum
- `ironmud://mobile/{vnum}` - Mobile data by vnum

## Development

```bash
# Watch mode for development
npm run dev

# Build for production
npm run build

# Start the server
npm start
```

## Important Notes

1. **Spawn Points are Critical**: Mobiles and items without spawn points will NOT respawn after death/pickup. Always create spawn points for persistent content.

2. **Building Order**: When creating an area, follow this order:
   1. Create the area (establishes prefix)
   2. Create rooms with vnums using the area prefix
   3. Connect rooms with exits
   4. Create item and mobile prototypes
   5. Create spawn points
   6. Add spawn dependencies for mobile equipment

3. **Permissions**: API keys are linked to characters. Area permissions determine what content you can modify.

4. **Description Generation**: Use the description context tools (`get_room_context`, `get_item_context`, `get_mobile_context`) before writing descriptions. They provide area theme, suggested sensory elements, and style guidance.

## Troubleshooting

### "IRONMUD_API_KEY is required"
Make sure the environment variable is set correctly.

### "Connection refused"
Ensure IronMUD is running with the REST API enabled:
```bash
IRONMUD_API_ENABLED=true cargo run
```

### "Forbidden" errors
Check that your API key has the correct permissions (read/write) and that you have permission to edit the target area.
