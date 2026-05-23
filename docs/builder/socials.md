# Social Actions

CircleMUD/tbaMUD-style social commands. Each social is a verb players
type to broadcast a rendered message to themselves, the room, and an
optional target. Stock IronMUD ships ~490 socials imported from tbamud
(`smile`, `wave`, `bow`, `dance`, `hug`, `nod`, `grin`, `kiss`, …); the
table loads from `scripts/data/socials.json` at server startup.

## For Players

Type a social verb to broadcast it. Common shapes:

```text
> wave
You wave happily.
Bob waves happily.

> wave alice
You wave at Alice.
Bob waves at Alice.            (others see)
Bob waves at you.              (Alice sees)

> wave self                   (or `wave me`)
You wave at yourself.
Bob waves at himself.

> wave gribble
You don't see them here.
```

- Type `socials` to see the full list of loaded social verbs. The main
  `help` listing intentionally omits them so it stays scannable; only the
  `socials` entry shows up there.
- Tab completion still resolves social prefixes (e.g. `wa<TAB>` → `wave`)
  alongside built-in commands.
- Each social has a position requirement (most need standing; some like
  `groan` work while sitting). Sleeping blocks almost every social.
- A handful of socials are flagged `hide=true` and only render to the
  actor and victim — bystanders never see them. Inherited from Circle.

## Data Format

`scripts/data/socials.json` is the single source of truth. Fresh installs
populate it by running the CircleMUD/tbaMUD importer (see
[Import Guide](../import-guide.md)).

Each entry:

```jsonc
{
  "name": "smile",
  "abbrev": "smi",
  "hide": false,
  "min_victim_position": "sleeping",
  "min_char_position": "sleeping",
  "min_level": 0,
  "char_no_arg":   "You smile happily.",
  "others_no_arg": "$n smiles happily.",
  "char_found":    "You smile at $N.",
  "others_found":  "$n smiles at $N.",
  "vict_found":    "$n smiles at you.",
  "not_found":     "You don't see them here.",
  "char_auto":     "You smile at yourself.",
  "others_auto":   "$n smiles at $mself.",
  "tags": ["content", "greeting"]
}
```

### Pronoun tokens

Lowercase = actor, uppercase = victim/secondary:

| Token        | Resolves to                              |
|--------------|------------------------------------------|
| `$n` / `$N`  | name (`someone` when hidden + invisible) |
| `$e` / `$E`  | subjective: he/she/they/it               |
| `$m` / `$M`  | objective: him/her/them/it               |
| `$s` / `$S`  | possessive: his/her/their/its            |
| `$mself` / `$Mself` | reflexive: himself/herself/themself/itself |
| `$p` / `$P`  | object short-desc (reserved)             |
| `$t` / `$T`  | body-part / free-text (reserved)         |
| `$$`         | literal `$`                              |

### Position values

`sleeping` (anyone can do it) → `sitting` → `standing`. CircleMUD's
nine-rank position ladder collapses onto this three-bucket scheme:
DEAD/MORT/INCAP/STUN/SLEEPING/RESTING → `sleeping`,
SITTING → `sitting`, FIGHTING/STANDING → `standing`.

### Tags

Optional `tags` array steers NPC ambient-emote selection in the
simulation tick. A simulated NPC with low energy might surface `yawn`
(tagged `tired`); a depressed NPC might `sigh` (tagged `depressed`).
Untagged socials are still fully usable by players — they just don't
surface in sim-driven emotes.

Recognised tag values:
`content`, `sad`, `depressed`, `breakdown`, `grief`, `hungry`, `tired`,
`uncomfortable`, `idle`, `greeting`, `farewell`, `affection`,
`aggression`, `comfort`.

A baseline of ~40 well-known socials are auto-tagged at load time
(`sigh`→sad+depressed, `weep`→depressed+breakdown+grief, …); see
`auto_tag` in `src/social/actions.rs`. Authored tags in the JSON take
precedence — auto-tagging only fires when the field is empty.

## DG Scripts

Triggers can fire socials via the `social` verb:

```dg
* greet anyone who enters
> Greeting~
social wave %actor%
social smile %actor%
~
```

- Only mob triggers can fire socials — there's no actor to render for
  room or item triggers. A `social` invocation from a non-mob context
  warns the builder and no-ops.
- Pronouns resolve from the mob's `characteristics.gender` (set via
  `medit <id> char gender`).
- Position gates apply: a sleeping mob can't `wave`. Failures silently
  no-op, matching tbamud's behavior on misconfigured commands.

## Importing

To refresh the table from a tbamud or CircleMUD distribution:

```bash
ironmud-import --database ironmud.db circle \
  --source /path/to/tbamud --apply
```

The importer reads `<source>/lib/misc/socials.new` (or legacy
`socials`), parses every record, and overwrites `scripts/data/socials.json`.
Restart the server to pick up changes — the file is loaded once at
startup.
