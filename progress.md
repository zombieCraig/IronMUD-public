## Progress Log: IronMUD Development

**Objective:** Build a fully-featured Multi-User Dungeon (MUD) engine with character system, rooms, movement, and social features.

---

### **Status: Core Features COMPLETE** ✓

The character system, room system, movement, and social features are fully functional.

---

### **Key Implementation Details:**

#### Architecture Changes Made:

1. **Separated `SharedConnections` from `SharedState`**
   - `SharedConnections = Arc<Mutex<HashMap<ConnectionId, PlayerSession>>>`
   - This prevents deadlock when Rhai scripts call registered functions
   - The `World` struct now holds `connections: SharedConnections` (an Arc clone)
   - Rhai functions receive `SharedConnections` directly, not `SharedState`

2. **Added `scripts` field to World struct**
   - `scripts: HashMap<String, AST>` - caches compiled Rhai scripts
   - Added `load_scripts()` function to compile all scripts on startup
   - `watch_scripts()` recompiles on file change for hot-reloading

3. **CharacterData Rhai Integration**
   - Registered getters/setters for `name`, `password_hash`, `current_room_id`
   - Added `new_character(name, password_hash, room_id)` constructor for Rhai
   - `current_room_id` is stored as Uuid but exposed as String to Rhai

4. **Script Execution Without Deadlock**
   - In `handle_connection`, we clone the AST out and release the World lock BEFORE calling `engine.call_fn`
   - Registered Rhai functions can then safely lock `SharedConnections`

#### Key Files:

| File | Purpose |
|------|---------|
| `src/lib.rs` | Core types: CharacterData, RoomData, PlayerSession, OnlinePlayer; helper functions |
| `src/db.rs` | Sled database: character/room CRUD, password hashing, migrations |
| `src/script.rs` | Rhai function registrations for all game operations |
| `scripts/commands/*.rhai` | Command implementations (create, login, logout, quit, look, exits, go, say, who, help, alias, unalias, emote, tell, whisper, shout) |
| `scripts/commands.json` | Command metadata and access control |

---

### **Working Features:**

#### Character System
1. **Character Creation** (`create <name> <password>`)
   - Validates input, checks for existing character
   - Hashes password with Argon2, saves to Sled database

2. **Character Login** (`login <name> <password>`)
   - Retrieves character, verifies password
   - Associates character with session, auto-looks at room
   - Announces spawn: "X appears in a flash of light."

3. **Character Logout** (`logout`)
   - Saves character, clears session
   - Returns to welcome banner
   - Announces departure: "X fades away into the ether."

4. **Quit** (`quit`)
   - Same as logout but also disconnects client

#### Room System
5. **Rooms** (RoomData with title, description, exits)
   - Six-direction exits: north, east, south, west, up, down
   - Seed rooms: Town Square, Rusty Tankard, Market Street
   - Automatic migration of characters with invalid room IDs

6. **Look** (`look` or `look <direction>`)
   - Shows room title, description, available exits
   - Lists other characters in the room
   - `look <direction>` shows the title of the room in that direction

7. **Exits** (`exits`)
   - Lists all exits with their destination room titles
   - Format: "North - The Rusty Tankard"

8. **Movement** (`go <direction>` or direction shortcuts)
   - Supports: north/n, east/e, south/s, west/w, up/u, down/d
   - Announces departure and arrival to other players
   - Auto-look on arrival

#### Social Features
8. **Say** (`say <message>`)
   - Broadcasts message to all players in the same room

9. **Who** (`who`)
   - Lists all online players with room name and IP address

10. **Emote** (`emote <action>` or `: <action>`)
    - Shows "You <action>" to the actor
    - Broadcasts "<name> <action>" to others in the room
    - Default alias `:` for quick emotes

11. **Tell** (`tell <player> <message>`)
    - Sends a private message to any online player
    - Works regardless of which room the target is in
    - Case-insensitive player name matching

12. **Whisper** (`whisper <player> <message>`)
    - Sends a private message to a player in the same room only
    - Fails if target is in a different room

13. **Shout** (`shout <message>`)
    - In current room: "<name> shouts: <message>!"
    - In adjacent rooms: "Someone shouts: <message>!"
    - Adjacent rooms = rooms reachable via any exit direction

#### System Features
10. **Command Access Control**
    - `guest`: Only available before login (create, login)
    - `user`: Only available after login (look, go, say, logout, alias, unalias)
    - `any`: Available anytime (quit, help, who)
    - `admin`: Reserved for admin commands (stubbed)

#### Alias System
11. **Alias Command** (`alias [name] [expansion]`)
    - No args: lists all aliases (user and default)
    - One arg: shows specific alias definition
    - Two+ args: creates/updates alias
    - Prevents aliasing reserved commands (`alias`, `unalias`)

12. **Unalias Command** (`unalias <name>`)
    - Removes user-defined aliases
    - Cannot remove default aliases (only override them)
    - Removing an override restores the default

13. **Default Aliases**
    - `n` => `go north`, `s` => `go south`, `e` => `go east`, `w` => `go west`
    - `'` => `say` (apostrophe for quick speech)

14. **Semicolon Command Chaining**
    - Multiple commands can be separated by `;`
    - Works in both aliases and direct input
    - Example: `look; say I'm here` executes both commands

15. **Alias Argument Appending**
    - Arguments passed to an alias are appended to each command
    - Example: if `g` => `go`, then `g north` becomes `go north`
    - For multi-command aliases: `combo arg` with `combo` => `look; say hi` becomes `look arg; say hi arg`

16. **Alias Persistence**
    - Aliases stored in `CharacterData.aliases` (HashMap<String, String>)
    - Saved to database, persists across sessions
    - Each character has their own alias set

---

### **Critical Lessons Learned:**

1. **Deadlock with std::Mutex**: Never hold a `Mutex` lock while calling code that might lock the same mutex. Rhai function closures capture state and try to lock when called during script execution.

2. **Rhai String Methods**: `trim()` is NOT a built-in Rhai string method. Use `== ""` for empty checks.

3. **Rhai Maps vs Registered Types**: Rhai maps (`#{...}`) are not the same as registered Rust types. Use a registered constructor function (`new_character()`) to create proper typed objects.

4. **Function Registration**: Must register Rhai functions BEFORE loading/compiling scripts, otherwise the functions won't be found at runtime.

5. **Rhai Expression Complexity**: Rhai has a maximum expression complexity limit. Long string concatenations like `"a" + b + "c" + d + "e"` can exceed this limit. Solution: break into multiple statements using `+=` or extract to helper functions.

---

### **Test Command:**

```bash
cargo test --test server
```

All 5 test scenarios pass:
1. Character creation
2. Successful login
3. Logout and disconnect
4. Login with wrong password
5. Login with non-existent character

---

### **Future Work:**

1. **Re-enable `matrix` and `web` modules** - Currently disabled due to outdated dependencies
2. **Admin commands** - Currently stubbed, needs implementation
3. **More rooms and areas** - Expand beyond the three seed rooms
4. **Combat system** - NPCs, monsters, and player combat
5. **Inventory system** - Items and equipment
