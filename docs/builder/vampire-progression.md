# Authoring the Sire Quest (Thinblood → Clan Progression)

When `enable_vampire_creation` is on, players who pick the vampire class at
character creation are auto-embraced as **thinbloods** — embraced kindred
with no clan trait. They retain a small mechanical handicap (max blood
pool 6, halved sun damage as a pro, halved humanity loss as a pro,
locked tier-3 disciplines as a con) until a clan acknowledges them.

This page describes the plumbing builders use to author a sire NPC and
clan-embrace quest. No sample content ships with the engine.

## Pieces you have

| Piece | What it does |
|---|---|
| `is_thinblood` dialogue condition | Branch on "embraced but no clan" |
| `is_clan_acknowledged` dialogue condition | Inverse — branch on clan-acknowledged kindred |
| `OfferQuest` dialogue effect | Standard quest hook from quests slice 1 |
| `EmbraceClan` quest reward | Stamps `clan_<name>` trait + 1-dot starter discipline + uplifts blood pool to 10 |

## Recipe

1. **Create the sire NPC.** A regular `MobileData`. Convention is to flag
   them `vampire` (so they show up to Auspex, take sun damage themselves,
   etc.) and place them in a sheltered (indoors) starter-zone room.

2. **Author a dialogue tree on the sire.** Three branches keyed on
   kindred status:

   - Root → choices:
     - `condition: is_thinblood` → branch that names the trial and offers
       the quest via `OfferQuest { vnum }`.
     - `condition: is_clan_acknowledged` → cordial recognition branch. No
       quest.
     - Default (mortals) → flavor-only branch.

   Build dialogue trees with the existing MCP tools
   (`add_mobile_dialogue`, `add_mobile_dialogue_node`,
   `add_mobile_dialogue_choice`) or `medit <id> tree ...` from the game.

3. **Author the embrace quest.** A regular `QuestData`. Set
   `giver_mob_vnum` to the sire's prototype vnum so the quest
   acknowledges the right sire when complete. Add objectives that suit
   the clan flavor (KillMob for Brujah brawls, BringItem for Toreador
   gifts, etc.). Add the embrace-clan reward last:

   ```
   quedit <quest_vnum> reward add embrace_clan brujah
   ```

   Or via MCP: `update_quest` with a `rewards` array entry like
   `{ "kind": "embrace_clan", "clan": "brujah" }`. Valid clan ids:
   `brujah`, `toreador`, `ventrue`, `nosferatu`, `gangrel`.

4. **Verify the loop.** With `enable_vampire_creation: on`, roll a
   vampire-class character. `score` shows
   `Kindred: Thinblood (no clan)`. Talk to the sire — only the thinblood
   branch should fire. Accept the quest, complete the objectives. On
   completion the player sees a flavor line, gains the clan trait, the
   first preferred discipline (per
   `scripts/data/vampire_clans.json` `preferred_disciplines`), and a
   full blood pool. `score` now shows `Kindred: <Clan>`.

## What the reward stamps

The `embrace_clan` reward is idempotent and resolves giver mob name from
the quest's `giver_mob_vnum`. On completion:

- Adds `clan_<name>` trait if missing.
- Seeds 1 dot of the clan's first preferred discipline (data-driven via
  `scripts/data/vampire_clans.json`). Skill stays untouched if already
  ≥ 1.
- Sets `vampire_state.sire_id` to the giver mob's display name.
- Raises `max_blood_pool` to 10 and refills `blood_pool` to 10.

The reward is a no-op (with a warning logged) if the player is mortal or
already clan-acknowledged when the quest completes.

## Admin escape hatch

Staff can short-circuit the progression with
`admin embrace <player> <clan>`. The handler calls the same
acknowledgment path, so the player ends up identical to a quest-driven
acknowledgment (clan trait, 1-dot discipline, blood-pool uplift).

`admin embrace_revoke <player>` clears the entire `vampire_state`
including the clan trait, returning the player to mortal.
