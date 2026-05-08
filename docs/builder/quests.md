# Quests

Quests are first-class data in IronMUD. A quest is a named prototype with a list of objectives, a list of rewards, optional gates (prerequisite quests, minimum skill, time limit), and optional dialogue-tree hooks. Players accept and complete quests almost exclusively through NPC dialogue trees — the `quest` command is the player's status view, not a menu.

This page covers authoring quests with `quedit` and wiring them into NPC conversations. For dialogue authoring see [Dialogue Trees](dialogue-trees.md); for the player-facing view see the [Player Guide](../player-guide.md#quests).

## Quest Commands

| Command | Description |
|---------|-------------|
| `quedit create <vnum> <name>` | Create a new quest prototype |
| `quedit <vnum>` / `quedit <vnum> show` | Show full quest detail |
| `quedit <vnum> delete` | Destroy the prototype |
| `qlist` / `quests` (admin) | List all quest prototypes |

Vnums are short identifiers like `bandits_1`, `mayor_intro`, or `qst:101` — anything that's unique. Player commands accept the keywords or the vnum.

## Identity & Metadata

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `name` | `quedit <vnum> name <text>` | Display name shown in `quests` list |
| `summary` | `quedit <vnum> summary <text>` | One-line hook shown in the quest log |
| `desc` / `description` | `quedit <vnum> desc <text>` | Long body shown on accept and `quest <name>` |
| `completion` | `quedit <vnum> completion <text>` | Shown on successful turn-in (before rewards print) |
| `keywords` | `quedit <vnum> keywords <kw1 kw2 ...>` | Lookup aliases for `quest <kw>` |
| `giver` | `quedit <vnum> giver <mob_vnum\|"">` | Canonical questgiver (used for hints; pass `""` to clear) |
| `repeatable` | `quedit <vnum> repeatable on\|off` | Allow re-acceptance after completion |

```
> quedit create bandit_camp Clear the Bandit Camp
Quest 'bandit_camp' created.

> quedit bandit_camp summary Drive the bandits out of the eastern hills.
> quedit bandit_camp giver town_guard_captain
> quedit bandit_camp keywords bandit camp eastern hills
```

## Objectives

Objectives are added one at a time. Players see real-time progress in their quest log, and matching events automatically update progress — you don't fire anything from a script.

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `objective add kill` | `quedit <vnum> objective add kill <mob_vnum> <count>` | Slay N mobs of a vnum |
| `objective add bring` | `quedit <vnum> objective add bring <item_vnum> <qty> [<return_mob>]` | Collect N items; if `<return_mob>` is given, the items are auto-consumed when the player `give`s them to that mob |
| `objective add visit` | `quedit <vnum> objective add visit <room_vnum>` | Enter a specific room |
| `objective add flag` | `quedit <vnum> objective add flag <var> <value>` | A DG var (player or mob scope) reaches the target value |
| `objective remove` | `quedit <vnum> objective remove <idx>` | Delete by 0-based index |

```
> quedit bandit_camp objective add kill bandit_brute 5
> quedit bandit_camp objective add bring bandit_banner 1 town_guard_captain
> quedit bandit_camp objective add visit eastern_hills:bandit_camp
```

The `bring` objective with a `return_mob` is the standard fetch-quest shape: the player `give`s the item to the named mob and progress increments automatically. Without a `return_mob`, the engine simply checks the player's inventory at completion time.

The `flag` objective is the escape hatch — wire any DG trigger to set a variable and the quest engine will pick it up. Useful for "destroy this object", "befriend this NPC", or any state change that doesn't fit the kill/bring/visit shape.

## Rewards

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `reward add gold` | `quedit <vnum> reward add gold <amount>` | Grant gold |
| `reward add item` | `quedit <vnum> reward add item <vnum> <qty>` | Grant items |
| `reward add skill` | `quedit <vnum> reward add skill <key> <amount>` | Award skill XP (100 XP = 1 level, capped at 10) |
| `reward add achievement` | `quedit <vnum> reward add achievement <key>` | Unlock an achievement |
| `reward add recipe` | `quedit <vnum> reward add recipe <recipe_id>` | Teach a crafting recipe |
| `reward remove` | `quedit <vnum> reward remove <idx>` | Delete by 0-based index |

```
> quedit bandit_camp reward add gold 100
> quedit bandit_camp reward add item iron_helmet 1
> quedit bandit_camp reward add achievement bandit_slayer
```

Rewards fire in order on a successful `CompleteQuest` dialogue effect. If an item reward fails to spawn (vnum missing), the player is told and the rest of the rewards still process.

## Gates: Prereqs, Min-Skill, Duration

These keep quests appropriately scoped and prevent low-level players from accepting endgame chains.

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `prereq` | `quedit <vnum> prereq <quest_vnum\|clear>` | Require an earlier quest's completion |
| `min_skill` | `quedit <vnum> min_skill <n>` | Sum of player's skill levels must be ≥ n (`0` clears) |
| `duration` | `quedit <vnum> duration <secs>` | Time limit in seconds from acceptance (`0` clears) |

Prereqs and min-skill are checked at offer time — the dialogue choice's `OfferQuest` effect silently no-ops if the gate fails (use a `hint` and a `QuestComplete` condition on the choice if you want a visible "come back when you've finished X" line).

`duration` enforces a per-quest time limit. The quest tick runs every 60 seconds and removes any active quests whose `started_at + duration_secs` has elapsed; the player gets a `[ Quest expired: <name> ]` message on their next input. Offline players are not swept — expiry checks resume on login.

## Dialogue Hooks

The link between quests and NPCs is the dialogue tree. Two kinds of nodes are typical: an **offer** branch and a **turn-in** branch. The engine guards them with conditions and applies the right effect when the choice is taken.

### Conditions

These dialogue conditions check quest state. Attach them to a `DialogueChoice` to make it appear (or vanish) based on what the player has done.

| Condition | Effect |
|-----------|--------|
| `QuestActive { vnum }` | Player has the quest in flight |
| `QuestComplete { vnum }` | Player has completed the quest at least once |
| `QuestCompletable { vnum }` | Active AND every objective satisfied — gate the "turn it in" line |

### Effects

These effects mutate quest state when a choice is selected.

| Effect | Behavior |
|--------|----------|
| `OfferQuest { vnum }` | Adds to active quests; no-op if already active or already completed (unless repeatable). Also enforces `prereq` and `min_skill` gates silently. |
| `CompleteQuest { vnum }` | Calls the canonical completion path — verifies objectives, prints `completion` text, fires all rewards, marks completed. No-op with player feedback if objectives aren't met. |
| `AbandonQuest { vnum }` | Drops the quest from active. Progress is lost. |

### A Worked Example

A questgiver with three relevant choices: "tell me about the bandits" (offer), "I've cleared them out" (turn-in, gated on `QuestCompletable`), and "I'm working on it" (mid-quest small talk).

```
> medit captain tree addnode greeting Captain Aldric nods. "Looking for work?"
> medit captain tree addnode bandit_brief The eastern hills are crawling with bandit scum. Five of their best fighters and their banner — that should send a message.
> medit captain tree addnode bandit_thanks You've done the town a service. Take this with our thanks.

> medit captain tree addchoice greeting bandits | Tell me about the bandits. | goto bandit_brief
> medit captain tree addchoice bandit_brief accept | I'll do it. | exit
> medit captain tree addchoice greeting turnin | I've cleared the bandit camp. | goto bandit_thanks
> medit captain tree addchoice greeting progress | I'm still hunting them. | exit
> medit captain tree addchoice greeting bye | Just passing through. | exit
```

You'd then attach effects and conditions through the JSON form (via `tree set` or the MCP tools — the inline `addchoice` covers structure but not conditions/effects):

- The `accept` choice on `bandit_brief` gets `effects: [{ kind: "offer_quest", vnum: "bandit_camp" }]`.
- The `turnin` choice on `greeting` gets `conditions: [{ kind: "quest_completable", vnum: "bandit_camp" }]` and `effects: [{ kind: "complete_quest", vnum: "bandit_camp" }]`. The choice is hidden until the player has actually killed the brutes and is carrying the banner.
- The `progress` choice on `greeting` gets `conditions: [{ kind: "quest_active", vnum: "bandit_camp" }]` — only shown while the player is mid-quest.

See [Dialogue Trees](dialogue-trees.md) for the full effect/condition schema.

## Party Kill Credit

If multiple players damaged a target, each one with non-zero damage gets credit toward `KillMob` objectives — you don't need to identify a "killing blow." The combat system tracks `damaged_by` per fight and spell damage from outside combat is credited too. Same applies to charmed mobs: damage they deal counts toward their owner's quests.

## Importing tbamud `.qst`

The CircleMUD/tbamud importer (`src/import/mapping/quests.rs`) translates `.qst` files into IronMUD quest prototypes. Coverage:

| tbamud type | IronMUD mapping |
|-------------|-----------------|
| `AQ_OBJ_FIND` | `BringItem` (no return mob) |
| `AQ_OBJ_RETURN` | `BringItem { return_to_mob_vnum: Some(qm) }` |
| `AQ_ROOM_FIND` | `VisitRoom` |
| `AQ_MOB_KILL` | `KillMob` |

Unsupported (warn-only, no row produced):

- `AQ_MOB_FIND` (find mob alive — would need a tracker)
- `AQ_MOB_SAVE` (rescue mechanic — not modelled)
- `AQ_ROOM_CLEAR` (clear all mobs from room — deferred)

Gold and item rewards survive the round-trip; achievements, skill XP, recipes, prereqs, min-skill, and durations don't exist in stock tbamud and are left empty for the builder to fill in. The importer maps the tbamud "quest master" to `giver_mob_vnum` and the `repeatable` flag is honored.

## Related Documentation

- [Dialogue Trees](dialogue-trees.md) — wiring quests into NPC conversations
- [Mobiles](mobiles.md) — quest givers, dialogue surfaces
- [DG Scripts](dg-scripts.md) — using DG triggers + the `flag` objective for unusual progress conditions
- [Player Guide](../player-guide.md#quests) — what players see
