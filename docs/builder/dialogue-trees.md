# Dialogue Trees

Dialogue trees are the structured, branching alternative to the simple keyword `dialogue` system covered in [Mobiles](mobiles.md#dialogue-system). A tree turns a conversation into a graph of named nodes (each with text the NPC says and a list of player-visible choices), where choices can carry conditions, effects, cooldowns, and per-player limits. Trees integrate with quests, achievements, DG scripts, and the language system.

This page is the reference for everything you can put in a tree. For the basic `medit <id> tree` subcommands and a quick example, see the section in [Mobiles](mobiles.md#dialogue-trees).

## Data Model

A tree has two top-level fields:

- `root_node` — name of the entry node. When a player initiates dialogue, they always start here.
- `nodes` — a map of `name → DialogueNode`.

Each **node** has:

- `text` — what the NPC says when the player enters the node.
- `choices` — array of `DialogueChoice`. Order matters; that's the order players see them in.
- `on_enter` — array of `DialogueEffect`. Fires only on the player's *first* visit to this node.
- `on_each_visit` — array of `DialogueEffect`. Fires on every visit, including the first.
- `on_exit` — array of `DialogueEffect`. Fires when the player leaves the node (selecting a `goto` or `exit` choice).

Each **choice** has:

- `keyword` — the lookup word (also used as the menu shortcut).
- `label` — the menu line shown to the player.
- `target` — `goto: <node>` / `exit` / `repeat`.
- `conditions` — array of `DialogueCondition`. All must pass for the choice to be available.
- `effects` — array of `DialogueEffect` fired when the choice is taken (before navigation).
- `hint` — optional string. Shown in place of the label when the choice is locked by conditions; if absent, the locked choice is silently hidden.
- `cooldown_secs` — optional integer. The choice is unavailable until this many seconds have elapsed since the last pick.
- `once_per_player` — bool. After first pick, the choice vanishes for that player (per mob vnum, so different copies of the same prototype share the limit).

Per-player state (current node, visit counts, choice cooldowns, once-per-player picks) is tracked on `CharacterData` keyed by mob vnum, so two players talking to two copies of the same merchant see independent state.

## Conditions

Conditions are evaluated when classifying a choice for display. All conditions on a choice must pass for it to appear (or it locks/hides depending on `hint`).

| Kind | Fields | True when |
|------|--------|-----------|
| `flag_set` | `name`, `scope` (`local`/`global`) | A dialogue flag is set. `local` is scoped to the mob vnum; `global` is server-wide. Flags are toggled by `set_flag`/`clear_flag` effects. |
| `flag_unset` | `name`, `scope` | Inverse of `flag_set`. |
| `has_item` | `vnum`, `qty` (default 1) | Player carries qty of this item (any container counts). |
| `skill_at_least` | `key`, `level` | Player's skill level (0–10) on `key` meets the threshold. |
| `counter_at_least` | `key`, `value` | An achievement counter has reached `value`. |
| `dg_var_equals` | `scope` (`player`/`mob`), `key`, `value` | DG variable matches the string. Use this to bridge from arbitrary DG triggers. |
| `quest_active` | `vnum` | Player has this quest in their active list. |
| `quest_complete` | `vnum` | Player has completed this quest at least once. |
| `quest_completable` | `vnum` | Quest is active AND every objective is satisfied — gate the "I'm ready to turn it in" branch. |

## Effects

Effects fire when a choice is selected, in declared order. They also fire from node `on_enter` / `on_each_visit` / `on_exit` hooks.

| Kind | Fields | Behavior |
|------|--------|----------|
| `set_flag` | `name`, `scope` | Sets a dialogue flag. |
| `clear_flag` | `name`, `scope` | Clears a flag. |
| `give_item` | `vnum`, `qty` | Spawns and gives. Tells the player on failure (e.g., bad vnum) but doesn't block subsequent effects. |
| `take_item` | `vnum`, `qty` | Removes from the player's inventory. No-op message if the player doesn't have the items. |
| `award_skill_xp` | `skill`, `amount` | 100 XP = 1 level, capped at level 10. |
| `set_counter` | `key`, `value` | Sets an achievement counter. |
| `increment_counter` | `key`, `by` | Adds to a counter. |
| `set_dg_var` | `scope` (`player`/`mob`), `key`, `value` | Writes a DG variable. Pair with `dg_var_equals` conditions or with the quest `flag` objective. |
| `fire_dg_trigger` | `trigger_type`, `arg` | Manually fires a DG trigger on the mob — for hooks beyond the dialogue layer. |
| `offer_quest` | `vnum` | Adds the quest to the player. Silent no-op if already active or already completed (and not repeatable), or if prereqs/min-skill fail. |
| `complete_quest` | `vnum` | Calls the canonical completion path. No-op with feedback if objectives aren't met. |
| `abandon_quest` | `vnum` | Removes the quest from active. Progress lost. |

## Authoring with `medit tree`

The OLC editor lives in `medit`. The full subcommand set:

| Subcommand | Effect |
|------------|--------|
| `medit <id> tree show [<node>]` | Print the full tree JSON, or one node summary |
| `medit <id> tree nodes` | List node names |
| `medit <id> tree addnode <name> <text>` | Create a node. The first node added becomes root. |
| `medit <id> tree delnode <name>` | Remove a node (must not be root or referenced by any choice) |
| `medit <id> tree setroot <name>` | Re-point root_node |
| `medit <id> tree text <node> <new text>` | Inline edit of node text |
| `medit <id> tree edittext <node>` | Open the multi-line OLC editor on a node's text (`.h` for help, `.save`/`.abort` to commit/cancel) |
| `medit <id> tree addchoice <node> <kw> \| <label> \| <goto\|exit\|repeat> [target_node]` | Add a choice. Fields are pipe-delimited. |
| `medit <id> tree delchoice <node> <index>` | Remove a choice by 0-based index |
| `medit <id> tree editchoice <node> <index> hint <text>` | Set/clear hint (empty clears) |
| `medit <id> tree editchoice <node> <index> cooldown <secs>` | Set cooldown (0 clears) |
| `medit <id> tree editchoice <node> <index> once <on\|off>` | Toggle once_per_player |
| `medit <id> tree addcond <node> <choice_idx> <kind> [args]` | Append a condition to a choice |
| `medit <id> tree delcond <node> <choice_idx> <cond_idx>` | Remove a condition by index |
| `medit <id> tree addfx <node> <choice_idx> <kind> [args]` | Append an effect to a choice |
| `medit <id> tree delfx <node> <choice_idx> <fx_idx>` | Remove an effect by index |
| `medit <id> tree set <json>` | Replace the whole tree from JSON |
| `medit <id> tree clear` | Remove the tree entirely |

`tree show <node>` lists every condition and effect on each choice with its index, so you can verify what you've attached or pick the right `cond_idx` / `fx_idx` for deletion.

### Inline condition kinds (`addcond`)

| Kind | Args | Notes |
|------|------|-------|
| `flag_set` | `<name> [local\|global]` | Scope defaults to `local`. |
| `flag_unset` | `<name> [local\|global]` | |
| `has_item` | `<vnum> [qty]` | `qty` defaults to 1. |
| `skill_at_least` | `<skill_key> <level>` | |
| `counter_at_least` | `<counter_key> <value>` | |
| `quest_active` | `<quest_vnum>` | |
| `quest_complete` | `<quest_vnum>` | |
| `quest_completable` | `<quest_vnum>` | |
| `has_achievement` | `<achievement_key>` | |

### Inline effect kinds (`addfx`)

| Kind | Args | Notes |
|------|------|-------|
| `set_flag` | `<name> [local\|global]` | |
| `clear_flag` | `<name> [local\|global]` | |
| `give_item` | `<vnum> [qty]` | |
| `take_item` | `<vnum> [qty]` | |
| `award_skill_xp` | `<skill_key> <amount>` | 100 XP = 1 level (cap 10). |
| `set_counter` | `<counter_key> <value>` | |
| `increment_counter` | `<counter_key> [by]` | `by` defaults to 1. |
| `offer_quest` | `<quest_vnum>` | |
| `complete_quest` | `<quest_vnum>` | |
| `abandon_quest` | `<quest_vnum>` | |

Rarer kinds (`dg_var_equals` / `set_dg_var` / `fire_dg_trigger`, the vampire-specific conditions, `quest_choice_equals` / `set_quest_choice`) plus node-level hooks (`on_enter` / `on_each_visit` / `on_exit`) aren't covered by inline subcommands. Use:

1. **`tree set <json>`** — pass the full JSON document. Useful for bulk imports, copy-paste, and the kinds the inline editor doesn't cover.
2. **MCP tools** — programmatic authoring (see below). Recommended for quest writers building many trees at once.

## Authoring via MCP

The MCP server exposes granular tools that mirror the in-game commands but accept structured arguments, so you can attach conditions and effects from the start:

| Tool | Notes |
|------|-------|
| `add_mobile_dialogue_node` | Create a node with text |
| `add_mobile_dialogue_choice` | Append a choice with full target |
| `update_mobile_dialogue_node` | Patch text and/or `on_enter` / `on_each_visit` / `on_exit` |
| `update_mobile_dialogue_choice` | Replace a choice in full (label, target, conditions, effects, hint, cooldown, once) |
| `remove_mobile_dialogue_node` | Validates: must not be root or referenced |
| `remove_mobile_dialogue_choice` | By 0-based index |
| `set_mobile_dialogue_tree_root` | Re-point root |

All MCP tools run the same validation as `tree set`: root must exist, every `goto` target must resolve, keywords must be non-empty.

## Runtime Behavior

When a player initiates dialogue (using `talk <mob>` or saying a tree-keyword), the engine:

1. Sets the player's `dialogue_partner_id` so subsequent input is routed to the tree (sticky mode).
2. Enters the root node, fires its `on_each_visit` then `on_enter` (first visit only) effects, prints the node text, and renders the menu.
3. Waits for input. The player types a keyword (e.g., `quest`) or a number; the engine matches it against `classify_choices`:
   - `once_per_player` already-picked choices are dropped entirely.
   - Conditions are evaluated. Failed-with-hint choices show as `(?) <hint>`; failed-no-hint choices vanish.
   - Cooldowns render as `(available in 3m) <label>`.
4. On a successful pick, the engine records the cooldown timestamp and `once` marker, fires the choice's `effects`, then navigates: `Exit` (clear sticky mode, fire `on_exit`), `Goto` (fire `on_exit` on source, `on_each_visit` + `on_enter` on target), or `Repeat` (re-render the current node).

Saying `bye` always exits cleanly. The player can always escape with another command — sticky mode doesn't block movement or other input — but they'll re-enter at the root next time they start a conversation.

## Language Integration

If the mob's `spoken_language` is set (see [Languages](languages.md)), node text and dialogue responses are filtered through the per-listener garble layer. The mechanism is invisible to the tree author: write your dialogue in plain text and the engine handles the rest.

## Worked Example

A guard captain who offers a quest, gates the turn-in branch on completion of the objectives, and acknowledges mid-quest progress without revealing the offer twice:

```json
{
  "root_node": "greeting",
  "nodes": {
    "greeting": {
      "text": "Captain Aldric nods. \"Looking for work?\"",
      "choices": [
        {
          "keyword": "bandits",
          "label": "Tell me about the bandits.",
          "target": { "kind": "goto", "node": "bandit_brief" },
          "conditions": [
            { "kind": "quest_complete", "vnum": "bandit_camp" }
          ],
          "hint": "(you've already taken care of that)"
        },
        {
          "keyword": "bandits",
          "label": "Tell me about the bandits.",
          "target": { "kind": "goto", "node": "bandit_brief" },
          "conditions": [
            { "kind": "quest_active", "vnum": "bandit_camp" }
          ],
          "hint": "(you're already on it)"
        },
        {
          "keyword": "bandits",
          "label": "Tell me about the bandits.",
          "target": { "kind": "goto", "node": "bandit_brief" }
        },
        {
          "keyword": "turnin",
          "label": "I've cleared the bandit camp.",
          "target": { "kind": "goto", "node": "bandit_thanks" },
          "conditions": [
            { "kind": "quest_completable", "vnum": "bandit_camp" }
          ],
          "effects": [
            { "kind": "complete_quest", "vnum": "bandit_camp" }
          ]
        },
        {
          "keyword": "bye",
          "label": "Just passing through.",
          "target": { "kind": "exit" }
        }
      ]
    },
    "bandit_brief": {
      "text": "The eastern hills are crawling with bandit scum. Five of their best fighters and their banner — that should send a message.",
      "choices": [
        {
          "keyword": "accept",
          "label": "I'll do it.",
          "target": { "kind": "exit" },
          "effects": [
            { "kind": "offer_quest", "vnum": "bandit_camp" }
          ]
        },
        {
          "keyword": "back",
          "label": "Anything else?",
          "target": { "kind": "goto", "node": "greeting" }
        }
      ]
    },
    "bandit_thanks": {
      "text": "You've done the town a service. Take this with our thanks.",
      "choices": [
        {
          "keyword": "bye",
          "label": "It was my pleasure.",
          "target": { "kind": "exit" }
        }
      ]
    }
  }
}
```

The three "bandits" choices on the greeting node are mutually exclusive: the engine picks the first whose conditions pass. Order matters — the `quest_complete` line shows for finished players, the `quest_active` line for in-progress, and the unconditional one for everyone else.

The `turnin` choice is gated on `quest_completable` — it's invisible until the player has actually killed five brutes and is carrying the banner. Selecting it fires `complete_quest`, which validates objectives, prints the quest's `completion` text, and grants rewards.

## Worked Example — OLC Only

The same captain, built without touching JSON. Assumes the quest `bandit_camp` already exists (see [Quests](quests.md)) and that the captain's mob keyword is `captain`.

```
medit captain tree addnode greeting Captain Aldric nods. "Looking for work?"
medit captain tree addnode bandit_brief The eastern hills are crawling with bandit scum. Five of their best fighters and their banner — that should send a message.
medit captain tree addnode bandit_thanks You've done the town a service. Take this with our thanks.

# Greeting choices, top-to-bottom: turn-in (gated), progress (gated), offer.
medit captain tree addchoice greeting turnin | I've cleared the bandit camp. | goto bandit_thanks
medit captain tree addcond  greeting 0 quest_completable bandit_camp
medit captain tree addfx    greeting 0 complete_quest    bandit_camp

medit captain tree addchoice greeting progress | I'm still hunting them. | goto bandit_brief
medit captain tree addcond  greeting 1 quest_active bandit_camp

medit captain tree addchoice greeting bandits | Tell me about the bandits. | goto bandit_brief

medit captain tree addchoice greeting bye | Just passing through. | exit

# On the brief node, "I'll do it." offers the quest and ends the conversation.
medit captain tree addchoice bandit_brief accept | I'll do it. | exit
medit captain tree addfx    bandit_brief 0 offer_quest bandit_camp

medit captain tree addchoice bandit_brief back | Anything else? | goto greeting

# The thanks node just needs an exit.
medit captain tree addchoice bandit_thanks bye | It was my pleasure. | exit
```

The order of `addchoice` calls matters: the engine walks the choices top-to-bottom and shows the first one whose conditions pass. `turnin` (index 0) only appears after the player has met every objective; `progress` (index 1) only shows while the quest is active; the unconditional `bandits` choice (index 2) is the fallback that lets a new player ask about the quest in the first place.

Verify the result with `medit captain tree show greeting`:

```
Node `greeting` [root]
  text: Captain Aldric nods. "Looking for work?"
  choices:
    0. [turnin] I've cleared the bandit camp. -> bandit_thanks
         if [0] quest_completable bandit_camp
         do [0] complete_quest bandit_camp
    1. [progress] I'm still hunting them. -> bandit_brief
         if [0] quest_active bandit_camp
    2. [bandits] Tell me about the bandits. -> bandit_brief
    3. [bye] Just passing through. [exit]
```

The index numbers in `if [0]` and `do [0]` are the `cond_idx` / `fx_idx` you'd pass to `delcond` / `delfx` to remove them — e.g. `medit captain tree delcond greeting 0 0` drops the `quest_completable` gate from the turn-in choice.

## Related Documentation

- [Mobiles](mobiles.md#dialogue-trees) — the quick reference for the inline `medit tree` workflow
- [Quests](quests.md) — the OfferQuest/CompleteQuest hooks in detail
- [DG Scripts](dg-scripts.md) — `set_dg_var` / `fire_dg_trigger` effects and `dg_var_equals` conditions
- [Languages](languages.md) — per-listener garbling of node text
