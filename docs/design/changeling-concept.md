# Changeling Concept — Glamour, Clarity, and the Mask

> **Status: concept only.** Nothing in this document is implemented. It exists
> so we can decide whether the changeling earns a slot on the task list, with
> the mechanical reuse mapped out in advance. Flagship source: *Changeling:
> the Lost* (Onyx Path, 2nd edition), the same way vampire ≈ V:tM, werewolf ≈
> W:tA, mutant ≈ Mutant: Year Zero, replicant ≈ Blade Runner RPG, synth ≈
> Alien RPG.

## 1. Pitch and theme fit

Someone — some *thing* — took them. What came back wears their face, sees the
strangeness under the world's skin, and pays for both gifts daily. Changelings
are the third **theme-agnostic class overlay** on the vampire/werewolf
pattern: `classes_changeling.json` behind an `enable_changeling_creation`
runtime gate, usable from modern or fantasy worlds.

What makes the changeling worth building is not the fae flavor — it's that
the **Glamour economy would be the first race/class mechanic that runs on the
NPC needs/mood simulation**. Vampires feed on HP, mutants on their own flesh;
changelings would feed on *feelings*, and IronMUD already simulates feelings.

## 2. Glamour economy (the resource)

**Pool:** `GlamourState { glamour, max_glamour (10), clarity, ... }` behind
`Option<ChangelingState>` on `CharacterData` — the established
`Option<XState>` pattern.

**Spenders:** Contracts (see §4). Glamour does not regenerate on its own —
like Mutation Points, every point is harvested.

**Harvest surface — the part that's new.** Simulated NPCs already carry
`happiness` (0–100, `src/types/social.rs`) driving `MoodState`
(Content/Normal/Sad/Depressed/Breakdown), plus `NeedsState` ticked by
`src/ticks/simulation.rs`. Two harvest verbs, both targeting a simulated NPC
in the room:

| Verb | Mechanic | Yield | Cost to the NPC |
|---|---|---|---|
| `harvest <npc>` (skim) | passive draw from a **Content** NPC | +1–2 Glamour, cooldown per NPC | −5 happiness — sips the joy without souring it |
| `provoke <npc>` (reap) | run a social/emote that spikes emotion (terrify, infuriate, inspire), then draw hard | +3–5 Glamour | −15 to −25 happiness, possible mood drop, NPC remembers you (existing `remembered_enemies` / relationship affinity hit) |

Skimming is the sustainable loop; reaping is the fast, ugly one — and reaping
is what erodes Clarity (§3). The social-variant dispatcher and per-pair
`Relationship.affinity` plumbing already exist to price the social fallout.

**Court/Hollow trickle (v3):** +1 Glamour per tick while in a court-flagged
room (the `baseline_office` room-flag pattern).

## 3. Clarity (the degradation track)

The sanity ledger, sibling to vampire humanity and cyberware Humanity:
0–10, default 7.

**Lost by:** reap-harvesting (the predator's discount: −1 on a damaging
reap), breaking the Mask in front of mortals (reuse the masquerade-broken
plumbing — contracts cast before non-simulated witnesses), touching cold
iron (item `categories: ["cold_iron"]` check on wield/wear), and long stays
in the Hedge (v3).

**Regained:** slowly, via anchor rituals — the replicant signature-item
`attune`/`focus` pair is a perfect template (a *mortal keepsake* instead of
a memento), plus quest rewards.

**At low Clarity:** perception bleeds. 3–4: occasional false room emotes
(the cosmetic-glitch pattern from the synth chassis tick). 1–2: WIS/INT
debuffs re-stamped per tick. 0: a **lost episode** — the breakdown table
(replicant `roll_breakdown`) with fae flavors: fugue (stun), terror (flee
lock), the Mists (forget — temporary skill debuff).

## 4. Contracts (the powers)

Spells in `spells_changeling.json`, every entry `requires_changeling: true`
— the third boolean beside `requires_vampire`/`requires_werewolf` (the
plumbing is now a fully established pattern: one field on SpellDefinition,
one filter line in `get_available_spells`). Gate skill: `wyrd`. Costs come
from the Glamour pool, NOT mana — unlike werewolf Gnosis, the whole point is
that the fuel is harvested from NPCs, so it must be its own pool with a
`glamour_cost` field (the `resolve_cost` precedent on ability definitions).

Starter sketch (4–5):

- **Mask of Superiority** — buff: CHA boost (a borrowed face).
- **Hearth's Warmth** — heal-over-time (Regeneration buff).
- **Cloak of Leaves** — Obfuscate-style concealment (effect type exists).
- **Biting Cold** — damage, cold type.
- **Read the Heart** — utility: reveals a target NPC's mood, needs, likes
  (a player-facing window into the sim — cheap and uniquely changeling).

## 5. Courts (the clans/tribes analog)

Seasonal courts in `changeling_courts.json` + `court_*` traits, exact
clan/tribe shape — `frenzy_dc_modifier`-style effects on existing generic
lanes:

- **Summer (Wrath):** `frenzy_dc_modifier: -1` — anger close to the skin.
- **Autumn (Fear):** harvest-from-fear yield bonus (new effect key consumed
  by the harvest verb only).
- **Winter (Sorrow):** `flee_bonus` — the court of going unnoticed.
- **Spring (Desire):** skim yield bonus.

Court obligation (recurring quest hooks) is v3; courts ship as traits first.

## 6. Mechanical reuse map

| Changeling mechanic | Existing infrastructure |
|---|---|
| Class overlay + runtime gate | `classes_vampire.json` / `runtime_class_enabled` (now covers vampire+werewolf; add one match arm) |
| `Option<ChangelingState>` | vampire/replicant/mutant/synth/werewolf state pattern |
| Contracts | SpellDefinition `requires_*` lane + spells JSON overlay loading |
| Harvest targets | `MobileData.social.happiness`, `MoodState`, `NeedsState` (`src/types/social.rs`, `src/ticks/simulation.rs`) |
| Social fallout of reaping | `Relationship.affinity`, `remembered_enemies` |
| Clarity episodes | replicant breakdown table + buff stamping |
| Anchor ritual | replicant `attune`/`focus` commands |
| Mask breaks | vampire `masquerade_broken` plumbing |
| Court traits | clan/tribe trait registry + generic trait-effect extraction |
| Glamour tick (decay/episodes) | blood/rage tick wrapper shape |

## 7. Scope tiers

- **v1 (minimum playable):** ChangelingState (Glamour + Clarity), class
  overlay + gate, `harvest` skim verb, 3 Contracts, status line, tick.
- **v2:** `provoke` reap verb + social fallout, Clarity episodes + anchor
  ritual, cold-iron checks, 2 more Contracts.
- **v3:** Courts + obligations, court-room trickle, the Hedge (a rot-level
  style room dimension), Mask-break witnesses.

## 8. Risks and open questions

1. **Sim-tick ownership.** The simulation tick commits `needs` (and
   happiness via the social layer) with a CAS update that copies *owned*
   fields onto a fresh DB read (`src/ticks/simulation.rs:80-100`). A harvest
   verb mutating happiness from the command path must either go through the
   same `db.update_mobile` CAS (safe) or risk being clobbered by an
   in-flight sim tick. **Decision needed:** add a small
   `change_mobile_happiness(id, delta)` chokepoint in `src/script/social.rs`
   that uses CAS — do not touch happiness anywhere else.
2. **Harvest griefing.** Reaping crashes NPC happiness → Depressed mobs stop
   working/conversing. That's the *point* (predation has visible cost), but
   per-NPC harvest cooldowns and the affinity hit need tuning so one player
   can't deaden a whole town. The migration/mood systems already model
   recovery, which helps.
3. **Density dependency.** Glamour only flows where simulated NPCs live. A
   changeling in a dungeon starves. Mitigations: court-room trickle (v3) or
   a small Glamour award on quest completion.
4. **Fantasy-theme fit is excellent; modern fit is "urban fae"** — both work
   with the theme-agnostic overlay, no race gating beyond
   `incompatible_races: ["synth", "bioroid", "replicant", "revenant"]`
   (augmented allowed? cold iron vs chrome is a flavor call — propose
   allowed, since chrome is steel, not cold-wrought iron).
