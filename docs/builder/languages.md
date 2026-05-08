# Languages

IronMUD ships with a per-skill language system. Each character carries a level (0–10) in every language they know, picks one as their currently spoken tongue, and has their speech garbled for listeners whose skill is too low to follow. NPCs can be flagged to speak a non-Common language — drow guards in Common-speaking towns, dwarvish merchants who only understand other dwarves, etc.

This page covers authoring languages, wiring them onto NPCs, and the runtime garble model.

## Defined Languages

Language definitions live in JSON files under `scripts/data/`:

- `languages_fantasy.json` — Common, Elvish, Dwarvish, Orcish
- `languages_modern.json` — Common, Street Slang, High Speak, Protocol

Each `LanguageDefinition` has the following fields:

| Field | Type | Notes |
|-------|------|-------|
| `key` | string | Lowercase canonical key (e.g., `elvish`). Used everywhere — references, skill keys, `medit language` arg. |
| `display_name` | string | Player-facing name (e.g., `Elvish`). |
| `description` | string | Flavor text shown by `languages` and `speak`. |
| `is_lingua_franca` | bool | Exactly one language per ruleset must have this `true` (currently `common`). Lingua francas bypass the garble layer entirely. |
| `phonetic_words` | string[] | Pool of fake words used to replace failed words during garbling. Aim for 30+ for good variety. |

### Adding a Language

1. Add a new entry to one of the language JSON files.
2. Update race / class definitions in `scripts/data/races_*.json` and `scripts/data/classes_*.json` to include the new key in any relevant `starting_languages` map.
3. Restart the server (language definitions are loaded at boot).

```json
{
  "key": "celestial",
  "display_name": "Celestial",
  "description": "The harmonic tongue of angels and good outsiders.",
  "is_lingua_franca": false,
  "phonetic_words": ["lumen", "sancta", "veritas", "..." ]
}
```

## Player Side

### Skill and Levels

Languages are stored as skills on `CharacterData.skills`. Each carries a `level` (0–10) and an `experience` count toward the next level (100 XP per level). Speaking a non-lingua-franca language gives the speaker +1 XP per utterance and listeners +1 XP if they already have at least level 1 — passive immersion training.

The `linguist` trait grants +50% language XP; `tongue_tied` applies a −35% penalty.

### Starting Languages

Race and class definitions carry `starting_languages: { <key>: <level> }`. A character inherits the union — duplicates take the higher value. Examples (from the stock fantasy ruleset):

- Elves start at `elvish: 10` and `common: 10`.
- Half-elves start at `elvish: 5` and `common: 10`.
- Dwarves start at `dwarvish: 10` and `common: 7`.

### Switching Languages

Players use `speak <language>` to switch the language they're currently speaking, and `languages` to list what they know. The currently spoken language is stored in `CharacterData.current_language` (defaults to `common`):

```
> languages
=== Languages ===
  common      level 10  (everyone speaks this)
  elvish      level  4
  orcish      level  1

Spoken via 'say'/'tell'/'whisper'/'shout'. Switch with 'speak <language>'.

> speak elvish
You begin speaking Elvish.
```

## NPC Side

### Setting an NPC's Language

`MobileData.spoken_language: Option<String>`. Empty/None means the NPC speaks the lingua franca (Common). Set with:

```
> medit drow_guard language elvish
Spoken language set to: Elvish.

> medit drow_guard language clear
Spoken language cleared (mob now speaks Common).

> medit drow_guard language
Speaks: Elvish
Usage: medit drow_guard language <key|clear>
Known languages: common, elvish, dwarvish, orcish
```

The mob's spoken language affects `say`, `tell`, `whisper`, `shout`, `ask`, and dialogue tree node text — everything the NPC speaks aloud. It does not affect emotes (which describe actions, not speech) or DG `mecho`/`zecho` admin output.

### Dialogue Trees

Dialogue node text is automatically routed through the language layer. You don't author per-language variants — the engine garbles the same text per listener at runtime. The `DialogueSayLine` carries the raw text plus the mob's language key, and `emit_mob_speech` distributes a per-listener copy through `garble_for_mob_listener`.

There's currently no syntax for "different responses based on which language the listener understands". That's intentionally deferred — write your dialogue once in plain text and let the garble layer work.

### Migrant Defaults

Immigration spawns currently don't seed a non-Common language. If you want a dwarvish-speaking enclave, set `spoken_language` on the migrant prototypes that the area's immigration variation chances roll up.

## Garble Algorithm

The core is in `src/script/lang.rs`. For each word in the speaker's text:

- If the language is a lingua franca, the word passes through unchanged.
- Otherwise, the word survives with probability `effective_skill / 10.0`. So skill 5 → 50% pass-through, skill 10 → fully comprehensible, skill 0 → almost everything garbled.
- Failed words are replaced by a random entry from the language's `phonetic_words` pool, with capitalization and trailing punctuation preserved.

The **effective skill** for a (speaker, listener) pair is `min(speaker_skill, listener_skill)` — comprehension is bottlenecked by the weaker party. A skill-10 elf saying "hello" to a skill-2 listener has only 20% per-word survival.

Two listener-side bypasses:

- Admins always hear plaintext (debug aid).
- Speakers of a lingua franca always hear plaintext (the speaker has already self-translated).

## Channels

Language filtering applies to:

| Channel | Filter applied |
|---------|----------------|
| `say` | Yes — per-listener |
| `shout` | Yes — per-listener, in the source room *and* adjacent rooms |
| `ask` | Yes — for the NPC's response |
| `talk` (dialogue tree text) | Yes — node text and choice labels |
| `tell` / `whisper` | Yes — recipient's skill is the gate |

Bypasses (no language filtering):

- `emote` — describes an action; broadcasts plaintext.
- `gtell` (group tell) — out-of-band coordination.
- DG `mecho`, `zecho`, `wecho` — admin/script output.

## Slice Status

Two slices are in. Slice 1 wired the data plumbing: language definitions, `LanguageDefinition` type, garble algorithm, race/class starting languages, the player's `current_language`, and the per-language skill XP path on `say`/`shout`. Slice 2 wired NPCs as language speakers: `MobileData.spoken_language`, `medit language`, `DialogueSayLine`, per-listener garble in `say`/`shout`/`ask`/`talk`/dialogue trees.

Deferred (not in either slice):

- Trigger templates that switch language mid-script.
- Area-level language defaults or migrant language seeding.
- Language trainer NPCs (no formal "learn" mechanic — XP is purely passive from speaking/listening).

## Related Documentation

- [Mobiles](mobiles.md#identity-and-behavior) — the `medit language` subcommand
- [Dialogue Trees](dialogue-trees.md) — how node text flows through the garble layer
- [Player Guide](../player-guide.md) — `speak` and `languages` commands
