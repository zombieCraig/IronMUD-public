# Administration Guide

This guide covers server administration for IronMUD.

## In-Game Administration

### Admin Commands

| Command | Description |
|---------|-------------|
| `setadmin <player> [on\|off]` | Grant or revoke admin access |
| `setbuilder <player> [on\|off]` | Grant or revoke builder access (when `builder_mode = granted`) |

Note: Admins cannot remove their own admin status in-game.

### Builder Mode Setting

The `builder_mode` setting controls how the `setbuilder` command works:

| Mode | Behavior |
|------|----------|
| `all` (default) | Any user can toggle their own builder status |
| `granted` | Only admins can grant/revoke builder to others |
| `none` | Builder management disabled (use admin utility) |

## Admin Utility

An external command-line utility is available for offline administration:

```bash
# Build the utility
cargo build --release --bin ironmud-admin

# Run it
./target/release/ironmud-admin [OPTIONS] <COMMAND>
```

### User Management

```bash
# List all users with permissions
ironmud-admin user list

# Grant/revoke admin privileges
ironmud-admin user grant-admin <name>
ironmud-admin user revoke-admin <name>

# Grant/revoke builder privileges
ironmud-admin user grant-builder <name>
ironmud-admin user revoke-builder <name>

# Password management
ironmud-admin user change-password <name>
ironmud-admin user require-password-change <name>

# Delete a character
ironmud-admin user delete <name>
```

### World Management

```bash
# Show entity counts (areas, rooms, items, mobiles, etc.)
ironmud-admin world info

# Clear all world data (keeps characters, settings, API keys)
# Requires typing CONFIRM. Demo world re-seeds on next server start.
ironmud-admin world clear
```

To bring legacy MUD content (CircleMUD, etc.) into IronMUD, use the
`ironmud-import` utility — see the [Import Guide](import-guide.md).

### Server Settings

```bash
# List all settings
ironmud-admin settings list

# Get a setting value
ironmud-admin settings get builder_mode

# Set a setting value
ironmud-admin settings set builder_mode <all|granted|none>

# Delete a setting
ironmud-admin settings delete <key>
```

#### Notable Settings

`settings list` displays every known key with its default. A few that commonly need tuning:

| Setting | Default | Purpose |
|---------|---------|---------|
| `builder_mode` | `all` | Who can toggle builder access (`all`, `granted`, `none`) |
| `motd` | (empty) | Message of the day shown at login |
| `recall_enabled` | `true` | Whether the `recall` command is available |
| `login_lockout_duration` | `600` | Seconds of failed-login lockout |
| `idle_timeout_secs` | `300` | Seconds before idle disconnect |
| `wander_chance_percent` | `33` | Per-tick chance a wander-eligible mobile moves |
| `rent_period_game_days` | `30` | Length of a rental period for properties |
| `min_attackable_age` | `0` | Minimum NPC age (in game days) a player can attack. Protects children when raised above 0 |
| `conception_chance_per_day` | `0.005` | Per-day pregnancy chance for simulated opposite-gender partners/cohabitants |
| `adoption_chance_per_day` | `0.10` | Per-day chance an orphaned migrant is adopted by eligible candidates |

See `ironmud-admin settings list` on a live install for the complete list, including regen rates, corpse decay, and mail settings.

### Database Path

When installed via `install.sh`, the wrapper at `/usr/local/bin/ironmud-admin` automatically uses the production database at `/opt/ironmud/data/ironmud.db`:

```bash
sudo ironmud-admin user list
```

To override the database path, use either the `-d` flag or the `IRONMUD_DATABASE` environment variable:

```bash
# CLI flag (highest precedence)
ironmud-admin -d /path/to/ironmud.db user list

# Environment variable
IRONMUD_DATABASE=/path/to/ironmud.db ironmud-admin user list
```

Precedence: CLI flag (`-d`) > `IRONMUD_DATABASE` env var > default (`ironmud.db`).

## Matrix Integration

IronMUD can connect to a Matrix room to announce game events and receive commands.

### Prerequisites

1. **Matrix homeserver** - A running Matrix server (Synapse, Conduit, etc.)
2. **Bot account** - Create a dedicated Matrix account for the bot
3. **Room** - Create or choose a room for game announcements

### Creating a Bot Account

On your Matrix server:

```bash
# For Synapse
register_new_matrix_user -c /path/to/homeserver.yaml http://localhost:8008
```

Choose a username like `ironmud_bot` or `mud_announcer`.

### Finding the Room ID

The room ID (not alias) is required. Find it in:
- **Element/Web client**: Room Settings → Advanced → "Internal room ID"
- **Format**: `!randomstring:your.server.com`

### Configuration

#### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `MATRIX_HOMESERVER` | Matrix server URL | `http://matrix.local:8008` |
| `MATRIX_USER` | Bot username (without @) | `ironmud_bot` |
| `MATRIX_PASSWORD` | Bot account password | `secretpassword` |
| `MATRIX_ROOM` | Room ID (not alias) | `!AbCdEf:matrix.local` |

#### For systemd installations

```bash
sudo systemctl edit --full ironmud
```

Add the Matrix variables:
```ini
Environment="MATRIX_HOMESERVER=http://matrix.example.com:8008"
Environment="MATRIX_USER=ironmud_bot"
Environment="MATRIX_PASSWORD=your_password_here"
Environment="MATRIX_ROOM=!roomid:example.com"
```

Then reload and restart:
```bash
sudo systemctl daemon-reload
sudo systemctl restart ironmud
```

#### For manual runs

```bash
export MATRIX_HOMESERVER="http://matrix.example.com:8008"
export MATRIX_USER="ironmud_bot"
export MATRIX_PASSWORD="your_password"
export MATRIX_ROOM="!roomid:example.com"
cargo run --release
```

### Matrix Features

**Game to Matrix:**
- Player login announcements
- Player logout announcements

**Matrix to Game:**

| Command | Description |
|---------|-------------|
| `!who` | List currently online players |
| `!tell <player> <message>` | Send message to player |
| `!help` | Show available commands |

### Troubleshooting Matrix

| Issue | Solution |
|-------|----------|
| "Matrix integration disabled" | Check all 4 environment variables are set |
| "Failed to create Matrix client" | Verify MATRIX_HOMESERVER URL |
| "Failed to log in to Matrix" | Check username/password |
| "Failed to join Matrix room" | Verify room ID; invite bot first |

### Disabling Matrix

Unset or remove the environment variables. IronMUD runs normally without Matrix.

## Discord Integration

IronMUD can connect to a Discord channel to announce game events and receive commands. This works alongside or independently of the Matrix integration -- both can be enabled at the same time.

### Prerequisites

1. **Discord account** - An account to create the bot application
2. **Discord server** - A server (guild) where the bot will operate
3. **Bot application** - Created via the Discord Developer Portal

### Creating a Bot Application

1. Go to the [Discord Developer Portal](https://discord.com/developers/applications)
2. Click **New Application** and give it a name (e.g., "IronMUD")
3. Go to the **Bot** section in the left sidebar
4. Click **Reset Token** and copy the token -- this is your `DISCORD_TOKEN`
5. Under **Privileged Gateway Intents**, enable **Message Content Intent**

### Inviting the Bot to Your Server

1. In the Developer Portal, go to **OAuth2 → URL Generator**
2. Under **Scopes**, select `bot`
3. Under **Bot Permissions**, select:
   - Send Messages
   - Read Message History
   - View Channels
4. Copy the generated URL and open it in your browser to invite the bot

### Finding the Channel ID

1. In Discord, go to **User Settings → Advanced** and enable **Developer Mode**
2. Right-click the channel you want the bot to use
3. Click **Copy Channel ID**

### Configuration

#### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `DISCORD_TOKEN` | Bot token from Developer Portal | `MTIz...abc` |
| `DISCORD_CHANNEL_ID` | Target channel ID | `1234567890123456789` |
| `DISCORD_AVATAR` | (Optional) Path to avatar image | `assets/discord_avatar.png` |

If `DISCORD_AVATAR` is not set, the bot will check for a file at `assets/discord_avatar.png` automatically.

#### For systemd installations

```bash
sudo systemctl edit --full ironmud
```

Add the Discord variables:
```ini
Environment="DISCORD_TOKEN=your_bot_token_here"
Environment="DISCORD_CHANNEL_ID=1234567890123456789"
```

Then reload and restart:
```bash
sudo systemctl daemon-reload
sudo systemctl restart ironmud
```

#### For manual runs

```bash
export DISCORD_TOKEN="your_bot_token_here"
export DISCORD_CHANNEL_ID="1234567890123456789"
cargo run --release
```

### Discord Features

**Game to Discord:**
- Player login announcements
- Player logout announcements
- Server startup/shutdown notifications

**Discord to Game:**

| Command | Description |
|---------|-------------|
| `!who` | List currently online players |
| `!tell <player> <message>` | Send message to player |
| `!help` | Show available commands |

Messages sent via `!tell` from Discord appear in-game with a `[Discord]` prefix (compared to `[Matrix]` for Matrix messages).

### Troubleshooting Discord

| Issue | Solution |
|-------|----------|
| "Discord integration disabled" | Check that both `DISCORD_TOKEN` and `DISCORD_CHANNEL_ID` are set |
| "Failed to create Discord client" | Verify the bot token is correct and not expired |
| "Invalid DISCORD_CHANNEL_ID format" | Channel ID must be a numeric value |
| Bot connects but doesn't respond | Enable **Message Content Intent** in Developer Portal |
| Bot responds but can't see messages | Ensure bot has View Channels + Read Message History permissions |

### Disabling Discord

Unset or remove the environment variables. IronMUD runs normally without Discord.

### Using Both Matrix and Discord

Both integrations can be enabled simultaneously. Game events (logins, logouts, broadcasts) are sent to both platforms. Each platform's `!tell` command tags messages with its platform name (`[Matrix]` or `[Discord]`) so players know where messages originate.

## AI Integration

IronMUD can use AI to assist builders with writing descriptions.

### Supported Providers

- **Claude** (Anthropic) - Requires Anthropic API key
- **Gemini** (Google) - Requires Google AI API key

**Important:** Only use one provider at a time.

### Configuration

#### Claude

| Variable | Required | Description | Default |
|----------|----------|-------------|---------|
| `CLAUDE_API_KEY` | Yes | Anthropic API key | - |
| `CLAUDE_MODEL` | No | Model to use | `claude-sonnet-4-20250514` |
| `CLAUDE_MAX_TOKENS` | No | Max response tokens | `1024` |

#### Gemini

| Variable | Required | Description | Default |
|----------|----------|-------------|---------|
| `GEMINI_API_KEY` | Yes | Google AI API key | - |
| `GEMINI_MODEL` | No | Model to use | `gemini-2.0-flash` |
| `GEMINI_MAX_TOKENS` | No | Max response tokens | `1024` |

#### For systemd installations

```bash
sudo systemctl edit --full ironmud
```

Add ONE of the following (not both):
```ini
# For Claude:
Environment="CLAUDE_API_KEY=sk-ant-api03-..."

# OR for Gemini:
Environment="GEMINI_API_KEY=AIza..."
```

### AI Features

AI assists with description writing in OLC editors:

| Mode | Description |
|------|-------------|
| `help <prompt>` | Generate a new description from prompt |
| `rephrase` | Reword an existing description |

Example:
```
> redit desc help a cozy tavern with a crackling fireplace
Generating description... please wait.

=== AI-Generated Description ===
[The generated description text]
================================
Accept this description? (y/n)
```

For rooms, AI also suggests extra descriptions with keywords.

### Troubleshooting AI

| Issue | Solution |
|-------|----------|
| "AI integration disabled" | Set either CLAUDE_API_KEY or GEMINI_API_KEY |
| "Both...are set" error | Remove one API key |
| API errors | Check key is valid and has credits |

### Disabling AI

Unset both `CLAUDE_API_KEY` and `GEMINI_API_KEY`. IronMUD runs normally without AI.

## Building API (MCP Integration)

IronMUD provides a REST API that enables external tools like Claude Code to create and modify game content through the Model Context Protocol (MCP).

### Architecture

```
Claude Code → MCP Server (TypeScript) → REST API (port 4001) → Database
```

### API Key Management

API keys authenticate external tools and link to a character for permission checking.

```bash
# Generate new API key
ironmud-admin api-key create --name "claude-code" --character "craig" --write

# List all API keys
ironmud-admin api-key list

# Show API key details (without revealing the key)
ironmud-admin api-key show <key-id>

# Revoke an API key
ironmud-admin api-key revoke <key-id>
```

### API Key Permissions

| Permission | Description |
|------------|-------------|
| `read` | Can read areas, rooms, items, mobiles |
| `write` | Can create and modify content |
| `admin` | Bypasses area permission checks |

By default, write operations respect IronMUD's area permission system—the API key's linked character must have appropriate builder permissions for the target area.

### MCP Server Configuration

The MCP server (TypeScript) requires these environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `IRONMUD_API_URL` | REST API base URL | `http://localhost:4001` |
| `IRONMUD_API_KEY` | API key for authentication | (required) |

### REST API Configuration

The REST API runs on port 4001. For production deployments, use a reverse proxy (nginx, caddy) with HTTPS.

### Security Considerations

- **API keys** are hashed with Argon2 before storage
- **Transport security** - Use HTTPS via reverse proxy in production
- **Permission enforcement** - All write operations check area permissions
- **Rate limiting** - Can be configured via reverse proxy

### Disabling the Building API

The REST API is an optional component. If not started, IronMUD runs normally without external building tools.

See [MCP Integration Design](design/MCP_INTEGRATION.md) for full technical details.

## Monitoring

### Viewing Logs

```bash
# For systemd installations
sudo journalctl -u ironmud -f

# For manual runs
# Logs go to stdout
```

### Checking Status

```bash
# Service status
sudo systemctl status ironmud

# Who's online (in-game)
> who
```

## Backup and Restore

### Automatic Backups

The install script creates timestamped backups before upgrades:
```
data.backup-20240115-143022/
```

### Manual Backup

```bash
# Stop the server first
sudo systemctl stop ironmud

# Copy the database directory
cp -r /opt/ironmud/ironmud.db /backup/location/

# Restart
sudo systemctl start ironmud
```

### Restore

```bash
sudo systemctl stop ironmud
rm -rf /opt/ironmud/ironmud.db
cp -r /backup/location/ironmud.db /opt/ironmud/
sudo systemctl start ironmud
```

## Related Documentation

- [Getting Started](getting-started.md) - Demo world walkthrough and first steps
- [Installation](installation.md) - Server setup
- [Builder Guide](builder-guide.md) - Creating content
