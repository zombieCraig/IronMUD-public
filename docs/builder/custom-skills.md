# Custom Skills

Builder-defined "fake" skills (e.g. `dancing_queen`, `pickpocket_finesse`)
let one builder publish a named integer attribute that other builders can
discover and reference from items, DG triggers, and the `lookup` command —
without touching Rust.

This is for world-flavor mechanics that don't deserve a first-class core
skill (combat/crafting/etc.) but do need stable storage and discoverability.

## Publishing a skill

```
lookup skill publish dancing_queen Skill in coordinated dance steps.
```

Rules:

- Keys must match `^[a-z][a-z0-9_]{1,31}$`.
- Keys can't collide with a core skill (`magic`, `melee`, `ranged`, ...).
- Author is auto-stamped from the publisher's character name.
- Only the original author (or an admin) can unpublish.

To remove:

```
lookup skill unpublish dancing_queen
```

To browse:

```
lookup skill                          # lists core + custom
lookup skill dancing_queen            # description + author + usage hints
```

## Per-entity values

Every character and every mob carries a `custom_skills` map (key → integer)
backed by the same persistence as the rest of `CharacterData`/`MobileData`.
Missing key reads as `0`.

Values are *only* writable through the registry — calls referencing an
unregistered key are silently rejected. That's the safety: a typo in a DG
trigger doesn't quietly create a phantom skill.

## Reading from DG Scripts

The standard call-form accessor:

```
%actor.skill(dancing_queen)%      # effective value (base + buffs), as a string
%victim.skill(dancing_queen)%
%self.skill(dancing_queen)%       # when self is a mob
```

Returns `"0"` for absent keys or unknown actors — graceful, mirrors the
other readers (`%actor.affect(...)%`, etc.).

## Writing from DG Scripts

```
skill_set <target> <key> <value>      # absolute write
skill_add <target> <key> <delta>      # relative add (saturating)
```

`<target>` accepts the standard DG resolution: `%actor%`, `%victim%`,
`%self%`, a player name, a mob UUID. Examples:

```
skill_set %actor% dancing_queen 0
skill_add %actor% dancing_queen 1
skill_set 11111111-2222-3333-4444-555555555555 dancing_queen 5
```

Unknown keys log a builder warning and no-op — they won't crash the trigger.

## Item affects (`oedit ... affect`)

To stamp a passive bonus from equipment, use the standard `affect add`
grammar with `custom_skill_boost` plus the published key as the tag:

```
oedit 8472 affect add custom_skill_boost 1 dancing_queen
```

On equip, an `ActiveBuff` is stamped on the wearer with
`source: "item:<uuid>"`. On unequip it's stripped, just like
`strength_boost`/`hit_bonus`/etc. — equipment APPLY parity.

## Player-visible surfacing

Custom skills with a non-zero base or non-zero buff show up at the end of
the `skills` command under `-- Custom (Builder-Defined) --`:

```
  -- Custom (Builder-Defined) --
  Dancing Queen  : 3 (+1)
  Lockpicking    : 1
```

The `status` command surfaces buff effects on the six core attributes (e.g.
`Dex: 7 (+2)` when wearing the dancing boots). Custom skills are kept on
`skills` rather than `status` to keep the latter compact.

## A worked example: dancing boots

```
lookup skill publish dancing_queen Skill in coordinated dance steps.

oedit 8472 set name dancing boots
oedit 8472 set type armor
oedit 8472 affect add dexterity_boost 2
oedit 8472 affect add charisma_boost 1
oedit 8472 affect add custom_skill_boost 1 dancing_queen
```

Then a dance-contest mob trigger can branch on it:

```
> trigger dg body 8500
* event=speech speech=dance
if %actor.skill(dancing_queen)% > 5
  emote breaks into thunderous applause.
  skill_add %actor% dancing_queen 1
else
  say You'll need more practice.
end
~
```

## What to use vs. plain `dg_vars`

| Need                                                 | Use                  |
|------------------------------------------------------|----------------------|
| Discoverability across builders / cross-area reuse   | **custom skill**     |
| Item APPLY equip/unequip stacking                    | **custom skill**     |
| Surfaced in `lookup skill` and `skills`              | **custom skill**     |
| One-shot trigger state, per-mob bookkeeping          | plain `dg_vars`      |
| Free-form strings, non-integer values                | plain `dg_vars`      |
