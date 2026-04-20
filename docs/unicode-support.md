# Unicode/Japanese Language Support Plan

## Executive Summary

This document covers Unicode (specifically Japanese: hiragana, katakana, kanji) support in IronMUD.

**Status:** Levels 1-2 are **COMPLETE** as of January 2026.
- **Level 1:** UTF-8 input parsing - players can type Japanese, accented characters, and emoji
- **Level 2:** MTTS client detection - server detects client capabilities including UTF-8 support

## Project Context

- **Priority:** Low - document now, implement when time permits
- **Target Audience:** Primarily English-speaking, with some users learning Japanese who would benefit from practice opportunities
- **Transport Priority:** Telnet is the primary access method
- **Scope Decision:** General UTF-8 support (Level 1) is relatively low lift and provides foundation for future Japanese content

## Research Findings

### Current Codebase Status

| Component | UTF-8 Ready | Notes |
|-----------|-------------|-------|
| Database (sled + JSON) | Yes | JSON serialization handles UTF-8 natively |
| Rhai scripting | Yes | Rust strings are UTF-8 internally |
| Output to clients | Yes | `.as_bytes()` on String preserves UTF-8 |
| Character/room descriptions | Yes | Stored as Rust String type |
| Input parsing | Yes | Multi-byte UTF-8 sequences handled (Level 1 complete) |
| Cursor positioning | Yes | Uses `unicode-width` crate for correct display width |

### Resolved Issue: UTF-8 Input Parsing

**Problem (now fixed):** The original `parse_key_byte()` function in `telnet.rs` assumed each byte mapped to one character. A Japanese character like `あ` (U+3042) encoded as 3 bytes (`E3 81 82`) was parsed as 3 separate invalid characters.

**Solution (implemented):** Added `Utf8(Vec<u8>, usize)` state to `EscapeState` enum. The parser now:
1. Detects UTF-8 lead bytes and determines expected sequence length
2. Accumulates continuation bytes until sequence is complete
3. Decodes the complete sequence to a Unicode character

### Telnet Client UTF-8 Support

**Protocol Standards:**
- **MTTS** (Mud Terminal Type Standard): Bit 4 (value 4) indicates UTF-8 support
- **RFC 2066 CHARSET**: Explicit charset negotiation, but limited client support

**Client Compatibility:**
| Client | UTF-8 Support |
|--------|--------------|
| Mudlet | Yes (CHARSET + MTTS) |
| PuTTY/terminal telnet | Yes (passes through) |
| TinTin++ | Yes (MTTS) |
| CMUD | Latin-1 default (poor) |
| KildClient | Latin-1 (poor) |
| MUSHclient | Latin-1 (poor) |

**Recommendation:** Default to UTF-8, use MTTS to detect client capability, fallback gracefully for legacy clients.

## Implementation Scope Options

### Level 1: UTF-8 Content Support - COMPLETE

**Status:** Implemented January 2026 (commit 728a2bb)

**Goal:** Allow Japanese characters in room descriptions, item names, dialogue, etc.

**Changes Made:**
1. `src/telnet.rs` - Added `Utf8(Vec<u8>, usize)` variant to `EscapeState` enum
2. `src/telnet.rs` - Modified `parse_key_byte()` to detect UTF-8 lead bytes and accumulate multi-byte sequences
3. Added 6 unit tests for UTF-8 parsing (2-byte, 3-byte Japanese, 4-byte emoji, error cases)

**What Works Now:**
- Builders can use Japanese in descriptions
- Players with UTF-8 clients see Japanese text correctly
- Players can type Japanese, accented characters, and emoji

**Known Limitations:**
- Legacy clients may see garbled text (Level 2 detection now available)
- No automatic language switching

**Display Width Handling:** Emoji and CJK characters now have correct cursor positioning in readline editing (uses `unicode-width` crate). This prevents garbled display when typing or editing lines containing wide characters.

---

### Level 2: Client Capability Detection - COMPLETE

**Status:** Implemented January 2026 (commit 2672bfd)

**Goal:** Detect UTF-8 support and adapt behavior per-client.

**Changes Made:**
1. `src/telnet.rs` - Added TTYPE subnegotiation constants (`TTYPE_IS`, `TTYPE_SEND`)
2. `src/telnet.rs` - Added MTTS capability flag constants (ANSI, VT100, UTF8, 256_COLORS, etc.)
3. `src/telnet.rs` - Updated `TelnetState` with MTTS fields (`ttype_stage`, `client_name`, `terminal_type`, `mtts_flags`, `utf8_supported`)
4. `src/telnet.rs` - Added helper functions (`build_ttype_send()`, `parse_ttype_is()`, `parse_mtts_flags()`)
5. `src/telnet.rs` - Updated `build_initial_negotiations()` to request TTYPE
6. `src/lib.rs` - Added WILL TTYPE handling to start MTTS 3-stage negotiation
7. `src/lib.rs` - Added TTYPE subnegotiation handler with full 3-stage state machine
8. Added 4 unit tests for MTTS parsing

**What Works Now:**
- Server detects client capabilities via MTTS 3-stage negotiation
- `utf8_supported` flag available in `TelnetState`
- Client info logged when MTTS negotiation completes
- MTTS flags captured: ANSI, VT100, UTF-8, 256 colors, mouse tracking, screen reader, proxy, truecolor

**MTTS Bit Flags:**
| Bit | Value | Name | Meaning |
|-----|-------|------|---------|
| 1 | 1 | ANSI | Supports ANSI color codes |
| 2 | 2 | VT100 | Supports VT100 interface |
| 3 | 4 | **UTF-8** | Uses UTF-8 character encoding |
| 4 | 8 | 256_COLORS | Supports 256 color palette |
| 5 | 16 | MOUSE_TRACKING | Supports xterm mouse tracking |
| 6 | 32 | OSC_COLOR_PALETTE | Supports OSC color palette |
| 7 | 64 | SCREEN_READER | Screen reader in use |
| 8 | 128 | PROXY | Connection is via proxy |
| 9 | 256 | TRUECOLOR | Supports 24-bit truecolor |

---

### Level 3: Japanese Command Input + Gradual Message Localization

**Goal:** Enable Japanese interaction - command input via aliases, system messages via gradual localization.

**Approach:** Two-phase implementation:
- **Phase 3A:** Add default Japanese command aliases (low effort, immediate value)
- **Phase 3B:** Build localization infrastructure and localize messages incrementally by tier

**Changes Required:**
1. Add Japanese command aliases to `get_default_aliases()` in `src/lib.rs`
2. Create `locales/en.json` and `locales/ja.json` message catalogs
3. Add `language: String` field to `CharacterData` and `TelnetState`
4. Create `get_msg(key)` Rhai function with fallback to English
5. Add `language` command for players to change preference
6. Gradually update Rhai scripts to use message keys (tiered approach)

**Localization Tiers:**
- Tier 1: Login/create flow (~80 strings)
- Tier 2: Core gameplay commands (~100 strings)
- Tier 3: Combat and systems (~80 strings)
- Tier 4+: Expand as needed

**What Works After:**
- Japanese speakers can type commands in Japanese immediately (Phase 3A)
- System messages display in Japanese based on player preference (Phase 3B)
- Easy to add more languages later

**Note:** Rhai complexity limits are NOT a concern - localization uses simple map lookups, not deeply nested conditionals.

---

### Level 4: Full Localization (Future)

**Goal:** Complete Japanese gameplay experience including builder content.

**Additional Requirements Beyond Level 3:**
- Help system localization (all help files)
- Builder content translation workflow (room descriptions, item names, NPC dialogue)
- Potentially dual-language content fields on game entities

**Complexity Notes:**
- CJK character width handling already implemented (`unicode-width` crate in place)
- IME (Input Method Editor) works via client - no server changes needed
- Builder content localization requires significant ongoing effort

---

## Implementation Status

### Phase 1: Foundation - COMPLETE

**Level 1** is implemented. This provides:
- Japanese characters work in all content
- ~120 lines added to `telnet.rs`
- No breaking changes to existing functionality
- Foundation for future expansion

### Phase 2: Detection - COMPLETE

**Level 2** is implemented. This provides:
- MTTS 3-stage negotiation to detect client capabilities
- `utf8_supported` flag per connection
- Client name and terminal type captured
- Full MTTS bitvector parsing (ANSI, 256 colors, truecolor, screen reader, etc.)
- Logging of client capabilities on connection

### Phase 3: Gradual Localization (Planned)

**Status:** Planned - viable path documented January 2026

**Goal:** Enable Japanese interaction via command aliases (input) and localized messages (output).

**Approach:** Gradual localization - start with high-impact areas (command input, login flow) and expand over time.

#### Feasibility Assessment

**Rhai Complexity Concern - NOT A BLOCKER**

The documented Rhai limitation concerns deeply nested if-else chains (8+ branches), not overall code size. Localization uses simple map lookups (`get_msg(key)`), not complex branching. Testing confirms message lookup functions work without hitting complexity limits.

**Scope Analysis:**
- 128 command scripts with ~500+ hardcoded English strings
- Login/create flow alone has ~80 localizable strings
- Existing alias system already supports UTF-8 (Japanese command input works today via user aliases)

#### Phase 3A: Japanese Command Aliases (Low Effort)

**Goal:** Let players type commands in Japanese without individual setup.

**Implementation:** Add default Japanese aliases to `src/lib.rs` `get_default_aliases()`:

```rust
// Japanese command aliases
defaults.insert("見る".to_string(), "look".to_string());
defaults.insert("言う".to_string(), "say".to_string());
defaults.insert("行く".to_string(), "go".to_string());
defaults.insert("北".to_string(), "go north".to_string());
defaults.insert("南".to_string(), "go south".to_string());
defaults.insert("東".to_string(), "go east".to_string());
defaults.insert("西".to_string(), "go west".to_string());
defaults.insert("上".to_string(), "go up".to_string());
defaults.insert("下".to_string(), "go down".to_string());
defaults.insert("取る".to_string(), "get".to_string());
defaults.insert("落とす".to_string(), "drop".to_string());
defaults.insert("攻撃".to_string(), "attack".to_string());
defaults.insert("持ち物".to_string(), "inventory".to_string());
defaults.insert("装備".to_string(), "equipment".to_string());
defaults.insert("助け".to_string(), "help".to_string());
```

**What This Achieves:**
- Japanese speakers can immediately use Japanese verbs for commands
- Zero changes to Rhai scripts required
- Players can still add their own aliases via `alias` command

#### Phase 3B: Localization Infrastructure (Medium Effort)

**Changes Required:**

1. **Locale Files** - Create `locales/en.json` and `locales/ja.json`:
   ```json
   {
     "login.usage": "Usage: login <character_name> <password>",
     "login.name_empty": "Character name cannot be empty.",
     "login.password_empty": "Password cannot be empty.",
     "login.not_found": "Character '{name}' not found.",
     "login.wrong_password": "Incorrect password.",
     "login.welcome": "Welcome, {name}!",
     "error.not_logged_in": "You must be logged in to do that."
   }
   ```

2. **Character Language Preference** - Add to `CharacterData` in `src/types/mod.rs`:
   ```rust
   #[serde(default)]
   pub language: String,  // "en", "ja", etc. - default "en"
   ```

3. **Connection-Level Language** - For pre-login messages, add to `TelnetState`:
   ```rust
   pub language: String,  // Default language for this connection
   ```

4. **Rhai Function** - Register `get_msg(key)` in `src/script/mod.rs`:
   - Retrieves player's language preference (or connection default)
   - Looks up key in appropriate locale file
   - Falls back to English if key not found
   - Supports `{placeholder}` substitution

5. **Language Command** - Create `scripts/commands/language.rhai`:
   - `language` - show current setting
   - `language ja` - switch to Japanese
   - `language en` - switch to English

#### Phase 3B Implementation Tiers

**Tier 1: Login/Create Flow (~80 strings)**
- `login.rhai` - login prompts, errors, welcome message
- `create.rhai` - character creation wizard
- Access control messages in `src/lib.rs`
- Password-related messages

**Tier 2: Core Gameplay (~100 strings)**
- `look.rhai` - room viewing messages
- `say.rhai`, `tell.rhai`, `whisper.rhai` - communication
- `get.rhai`, `drop.rhai`, `inventory.rhai` - item handling
- `go.rhai` - movement messages

**Tier 3: Combat & Systems (~80 strings)**
- `attack.rhai`, `flee.rhai` - combat messages
- `rest.rhai`, `sleep.rhai`, `wake.rhai` - status commands
- Death/respawn messages

**Tier 4+: Expand as needed**
- Shop system messages
- Help system
- Builder commands (likely stay English-only)

#### Language Selection Flow

**Before Login:**
1. Show language selection prompt: `[E]nglish / [日]本語`
2. Store choice in connection-level `language` field
3. Use for all pre-login messages

**After Login:**
1. Character's `language` field overrides connection default
2. Player can change with `language` command
3. Preference persists across sessions

#### What Stays English

- Builder/admin commands (`redit`, `medit`, `oedit`, etc.)
- Builder-created content (room descriptions, item names) - builders choose language
- Technical error messages and logs
- Help for builder commands

#### Maintenance Considerations

- New strings added in English first, Japanese translation follows
- Missing translations fall back to English (graceful degradation)
- Consider a translation workflow (export keys, translate, import)
- Track translation coverage percentage per locale

#### Help System Localization

**Principle:** Help should reflect the user's selected language, not clutter with both.

**Current Structure:**
- Command descriptions in `scripts/commands.json`
- Help displays via `help.rhai` using `get_available_commands()`

**Localized Help Approach:**

1. **Structured Command Descriptions** - Extend `commands.json`:
   ```json
   {
     "look": {
       "access": "user",
       "description": {
         "en": "Look at your surroundings or a direction",
         "ja": "周りを見る、または方向を見る"
       }
     }
   }
   ```

2. **Modify `get_available_commands()`** - Return descriptions in player's language:
   - Check player's `language` preference
   - Return localized description (fall back to English if missing)

3. **Extended Help Files** (Optional) - For detailed help:
   ```
   scripts/help/en/look.txt
   scripts/help/ja/look.txt
   ```

**Key Points:**
- Players see help ONLY in their selected language
- No bilingual clutter - clean, focused experience
- Builder/admin command help stays English-only
- Missing translations fall back to English gracefully

#### Phase 3C: Server-Side IME (Optional)

**Goal:** Allow Japanese input on systems without OS-level IME support.

**Problem:** Users on minimal systems (SSH from servers, old terminals) may lack Japanese IME. They can only type romaji (Latin characters).

**Solution:** Server-side romaji → kana conversion with space-triggered conversion.

**Rust Library:** [wana_kana](https://crates.io/crates/wana_kana) - Pure Rust, high performance (~1000 words/ms)

**How It Would Work:**

1. **IME Mode Toggle:**
   ```
   > ime on
   IME mode enabled. Romaji converts to kana on each space.
   > ime off
   IME mode disabled.
   ```

2. **Convert-on-Space Behavior:**
   ```
   > ime on
   > say konnichiha sekai
        ↓ (space triggers conversion)
   > say こんにちは せかい
   You say: こんにちは せかい
   ```

3. **Integration with Japanese Command Aliases:**
   ```
   > ime on
   > miru
     ↓ (Enter triggers final conversion)
   > 見る
     ↓ (alias expands)
   > look
   [Room description appears]
   ```

**Conversion Trigger Options:**

| Trigger | Behavior | Trade-off |
|---------|----------|-----------|
| On Enter | Convert entire line at once | Simple but no mid-line feedback |
| **On Space** | Convert each word as typed | Good balance - see conversion as you go |
| Real-time | Character-by-character | Complex, ambiguous mid-sequence |

**Recommendation:** Convert on Space - provides feedback during typing without the complexity of real-time conversion.

**Uppercase for Katakana:**
```
> say KONNICHIHA
You say: コンニチハ

> say Konnichiha    (mixed case)
You say: こんにちは  (defaults to hiragana)
```

**Implementation Approach (Option A - Kana Only):**

Since full kanji conversion (Option C) is unnecessary for a MUD context, the implementation uses only `wana_kana` for romaji → kana conversion. The curated "dictionary" is simply the existing alias system:

```
Japanese aliases provide the vocabulary:
  見る → look
  言う → say
  北 → go north

IME provides the input method:
  miru → 見る (via wana_kana)
  iu → 言う
  kita → きた (then alias handles if configured)
```

**Implementation Details:**

1. **Connection State** - Add to `TelnetState`:
   ```rust
   pub ime_enabled: bool,
   pub ime_buffer: String,  // Accumulates romaji between spaces
   ```

2. **Input Processing** - In readline handler:
   ```rust
   if ime_enabled {
       if byte == b' ' || byte == b'\r' {
           // Convert accumulated buffer
           let kana = ime_buffer.to_kana();
           // Replace buffer content with kana
           // Clear ime_buffer
       } else {
           // Accumulate in ime_buffer
           // Echo romaji character
       }
   }
   ```

3. **Rhai Functions:**
   ```rust
   engine.register_fn("to_kana", |s: &str| s.to_kana());
   engine.register_fn("set_ime_mode", set_ime_mode);
   engine.register_fn("get_ime_mode", get_ime_mode);
   ```

4. **IME Command** - `scripts/commands/ime.rhai`:
   ```rhai
   fn run_command(args, connection_id) {
       let current = get_ime_mode(connection_id);

       if args == "" {
           // Show current status
           if current {
               send_client_message(connection_id, "IME mode is ON (romaji → kana on space)");
           } else {
               send_client_message(connection_id, "IME mode is OFF");
           }
           return;
       }

       if args == "on" {
           set_ime_mode(connection_id, true);
           send_client_message(connection_id, "IME mode ON. Romaji converts to kana on each space.");
       } else if args == "off" {
           set_ime_mode(connection_id, false);
           send_client_message(connection_id, "IME mode OFF.");
       } else {
           // Direct conversion (utility)
           let kana = to_kana(args);
           send_client_message(connection_id, "→ " + kana);
       }
   }
   ```

**Visual Feedback During Typing:**

When IME is enabled, the server could optionally show conversion in progress:
```
> say k          (typing)
> say ko         (typing)
> say kon        (typing)
> say konn       (typing)
> say konni      (typing)
> say konnichi   (typing)
> say konnichiha (typing)
> say konnichiha (space pressed)
> say こんにちは  (converted, cursor after space)
```

This requires cursor repositioning but provides excellent feedback. Can be implemented as an enhancement after basic convert-on-space works.

## Implementation Details

### Level 1: UTF-8 Input Parsing

### Files Modified

1. **`src/telnet.rs`**
   - Added `Utf8(Vec<u8>, usize)` variant to `EscapeState` enum
   - Modified `parse_key_byte()` to detect UTF-8 lead bytes
   - Added UTF-8 state handler to accumulate continuation bytes
   - Added 6 unit tests for UTF-8 parsing

2. **`src/lib.rs`** - No changes needed
   - Existing code already accepts `char` from `KeyEvent::Char`

### UTF-8 Byte Pattern Reference

| Bytes | Pattern | Range |
|-------|---------|-------|
| 1 | 0xxxxxxx | U+0000 to U+007F (ASCII) |
| 2 | 110xxxxx 10xxxxxx | U+0080 to U+07FF |
| 3 | 1110xxxx 10xxxxxx 10xxxxxx | U+0800 to U+FFFF (includes Japanese) |
| 4 | 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx | U+10000 to U+10FFFF |

Japanese characters fall in the 3-byte range:
- Hiragana: U+3040-U+309F
- Katakana: U+30A0-U+30FF
- CJK (Kanji): U+4E00-U+9FFF

### Implementation Reference

```rust
pub enum EscapeState {
    Normal,
    GotEsc,
    GotCsi(Vec<u8>),
    Utf8(Vec<u8>, usize),  // (buffer, expected_length)
}

pub fn parse_key_byte(state: EscapeState, byte: u8) -> (EscapeState, Option<KeyEvent>) {
    match state {
        EscapeState::Utf8(mut buf, expected) => {
            if byte & 0xC0 == 0x80 {  // Continuation byte
                buf.push(byte);
                if buf.len() == expected {
                    // Complete - decode
                    if let Ok(s) = std::str::from_utf8(&buf) {
                        if let Some(c) = s.chars().next() {
                            return (EscapeState::Normal, Some(KeyEvent::Char(c)));
                        }
                    }
                    (EscapeState::Normal, Some(KeyEvent::Unknown))
                } else {
                    (EscapeState::Utf8(buf, expected), None)
                }
            } else {
                // Invalid continuation - reset and process this byte
                (EscapeState::Normal, Some(KeyEvent::Unknown))
            }
        }
        EscapeState::Normal => {
            // ... existing control/escape handling ...
            _ => {
                // Check for UTF-8 multi-byte lead
                let expected = if byte & 0x80 == 0 { 1 }
                    else if byte & 0xE0 == 0xC0 { 2 }
                    else if byte & 0xF0 == 0xE0 { 3 }
                    else if byte & 0xF8 == 0xF0 { 4 }
                    else { 0 };  // Invalid

                if expected == 1 {
                    // ASCII - handle directly
                    (EscapeState::Normal, Some(KeyEvent::Char(byte as char)))
                } else if expected > 1 {
                    // Start accumulating
                    (EscapeState::Utf8(vec![byte], expected), None)
                } else {
                    (EscapeState::Normal, Some(KeyEvent::Unknown))
                }
            }
        }
        // ... other states unchanged ...
    }
}
```

### Level 2: MTTS Client Detection

#### Files Modified

1. **`src/telnet.rs`**
   - Added TTYPE subnegotiation constants (`TTYPE_IS = 0`, `TTYPE_SEND = 1`)
   - Added MTTS capability flag constants (`MTTS_ANSI`, `MTTS_UTF8`, etc.)
   - Updated `TelnetState` struct with MTTS tracking fields
   - Added `build_ttype_send()` function
   - Added `parse_ttype_is()` function
   - Added `parse_mtts_flags()` function
   - Updated `build_initial_negotiations()` to include `DO TTYPE`
   - Added 4 unit tests for MTTS parsing

2. **`src/lib.rs`**
   - Added `OPT_TTYPE` case to `TelnetEvent::Will` handler
   - Added `OPT_TTYPE` case to `TelnetEvent::Subnegotiation` handler
   - Implemented 3-stage MTTS negotiation state machine

#### MTTS 3-Stage Negotiation Sequence

```
Server: IAC DO TTYPE                          (in initial negotiations)
Client: IAC WILL TTYPE
Server: IAC SB TTYPE SEND IAC SE              (stage 1 request)
Client: IAC SB TTYPE IS "MUDLET" IAC SE       (stage 1: client name)
Server: IAC SB TTYPE SEND IAC SE              (stage 2 request)
Client: IAC SB TTYPE IS "XTERM-256COLOR" IAC SE (stage 2: terminal type)
Server: IAC SB TTYPE SEND IAC SE              (stage 3 request)
Client: IAC SB TTYPE IS "MTTS 141" IAC SE     (stage 3: capability flags)
```

#### TelnetState Fields Added

```rust
pub struct TelnetState {
    // ... existing fields ...

    /// TTYPE negotiation stage (0=not started, 1-3=stage, 4=complete)
    pub ttype_stage: u8,
    /// Client name from TTYPE stage 1 (e.g., "MUDLET")
    pub client_name: Option<String>,
    /// Terminal type from TTYPE stage 2 (e.g., "XTERM-256COLOR")
    pub terminal_type: Option<String>,
    /// MTTS capability flags from stage 3
    pub mtts_flags: u32,
    /// Derived: client supports UTF-8 (from MTTS or assumed)
    pub utf8_supported: bool,
}
```

#### Usage Example

```rust
// Check if client supports UTF-8
if session.telnet_state.utf8_supported {
    // Send UTF-8 content
} else {
    // Fallback to ASCII or transliterate
}

// Check for 256 color support
if (session.telnet_state.mtts_flags & MTTS_256_COLORS) != 0 {
    // Use 256 color palette
}
```

## Testing

### Automated Tests (Implemented)

**Level 1 - UTF-8 parsing tests:**
- `test_parse_utf8_2byte` - European characters (é)
- `test_parse_utf8_3byte_japanese` - Hiragana (あ)
- `test_parse_utf8_katakana` - Katakana (カ)
- `test_parse_utf8_4byte_emoji` - Emoji (😀)
- `test_parse_utf8_invalid_continuation` - Error handling
- `test_parse_utf8_invalid_lead` - Error handling

**Level 2 - MTTS parsing tests:**
- `test_build_ttype_send` - Verify TTYPE SEND message format
- `test_parse_ttype_is` - Parse TTYPE IS responses
- `test_parse_mtts_flags` - Parse "MTTS nnn" strings
- `test_mtts_utf8_flag` - Verify UTF-8 flag detection from bitvector

### Manual Testing

**Level 1 - UTF-8 content:**
1. Connect with UTF-8 terminal (Mudlet, PuTTY with UTF-8)
2. Create room with Japanese description: `redit desc This is a test room. (テスト)`
3. Use `look` to verify display
4. Type Japanese in commands (e.g., `say こんにちは`)

**Level 2 - MTTS detection:**
1. Connect with Mudlet (supports MTTS)
2. Check server logs for "Client capabilities" message
3. Verify client_name, terminal_type, and mtts_flags are captured
4. Connect with plain `telnet localhost 4000` - should still work (no MTTS)

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Legacy client display issues | Medium | Low | Document UTF-8 requirement; Level 2 detection now available |
| Cursor positioning bugs | Very Low | Low | Fixed with unicode-width crate |
| Performance impact | Very Low | Low | UTF-8 decoding and MTTS negotiation are cheap |
| Breaking existing content | Very Low | Low | All changes are additive |
| MTTS negotiation timeout | Low | Very Low | Clients that don't respond simply don't get detected |

## Decision Points

1. **Is Japanese content needed now?** Level 1 foundation is complete. Content can be added as needed.

2. **What's the target audience?** Primarily English-speaking, but some users are learning Japanese and would benefit from practice opportunities.

## Next Steps

**Levels 1-2** are complete. The foundation is ready for Phase 3 when desired.

### Recommended Implementation Order

1. **Phase 3A: Japanese Command Aliases** (Quick win)
   - Add ~15 Japanese aliases to `get_default_aliases()` in `src/lib.rs`
   - Test with `cargo test` and manual verification
   - Immediate value: Japanese speakers can use Japanese commands

2. **Phase 3B-Infrastructure** (When ready to localize)
   - Create `locales/` directory and initial JSON files
   - Add `language` field to `CharacterData` and `TelnetState`
   - Register `get_msg()` Rhai function
   - Add `language` command

3. **Phase 3B-Tier1** (Login flow localization)
   - Extract login.rhai strings to locale files
   - Extract create.rhai strings
   - Add language selection at connection time

4. **Phase 3B-Help** (Help system localization)
   - Extend `commands.json` with localized descriptions
   - Update `get_available_commands()` to respect language preference
   - Optionally add extended help files per language

5. **Phase 3C: Server-Side IME** (Optional, for systems without OS IME)
   - Add `wana_kana` crate dependency
   - Register `to_kana()` Rhai function
   - Create `ime` command for romaji → kana conversion

6. **Continue tiers as needed...**

### Current Capabilities

The `utf8_supported` flag from MTTS can be used to:
- Filter/transliterate non-ASCII output for legacy clients
- Show/hide certain content based on client capability
- Debug charset-related player issues

### When to Proceed

Consider **Phase 3** when:
- There's demand for Japanese interaction (or desire to practice Japanese)
- Time available for initial infrastructure setup
- Commitment to translate priority messages

## Sources

- [Telnet Unicode Support - MUD Coders Wiki](https://mudcoders.fandom.com/wiki/Telnet:Unicode_Support)
- [MTTS Specification - TinTin++](https://tintin.mudhalla.net/protocols/mtts/)
- [Mudlet Supported Protocols](https://wiki.mudlet.org/w/Manual:Supported_Protocols)
- [RFC 2066 - TELNET CHARSET Option](https://www.rfc-editor.org/rfc/rfc2066.html)
- [Mudlet CHARSET Issue #637](https://github.com/Mudlet/Mudlet/issues/637)
- [Mudlet Unicode Manual](https://wiki.mudlet.org/w/Manual:Unicode)
