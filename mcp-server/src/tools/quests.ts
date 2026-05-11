// Quest tool definitions — fetch / kill / DG-flag quests.
//
// Quests are keyed by vnum (e.g. "qst:100"). Player progress lives on the
// character; this surface only authors the prototype. Slice 1 listeners
// auto-progress KillMob and BringItem (with `return_to_mob_vnum`) objectives;
// VisitRoom and DgFlag are data-only until slice 2.

const objectiveSchema = {
  oneOf: [
    {
      type: "object",
      properties: {
        kind: { const: "kill_mob" },
        vnum: { type: "string", description: "Mob prototype vnum to kill (e.g. '179')" },
        count: { type: "number", description: "How many to slay. Default 1." },
      },
      required: ["kind", "vnum"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "kill_any_mob" },
        vnums: {
          type: "array",
          items: { type: "string" },
          description:
            "Mob prototype vnums whose kills feed a shared counter. Use this for 'any of these' kill objectives (e.g. multi-vnum migrant hunters).",
        },
        count: { type: "number", description: "Total kills required across the set. Default 1." },
      },
      required: ["kind", "vnums"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "bring_item" },
        vnum: { type: "string", description: "Item prototype vnum to collect" },
        qty: { type: "number", description: "How many. Default 1." },
        return_to_mob_vnum: {
          type: "string",
          description:
            "If set, items are consumed automatically when the player gives them to this mob (auto-progress). If omitted, the player must invoke a CompleteQuest dialogue effect to turn in.",
        },
      },
      required: ["kind", "vnum"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "visit_room" },
        vnum: { type: "string", description: "Room vnum (data shape only — listener defers to slice 2)" },
      },
      required: ["kind", "vnum"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "dg_flag" },
        var: { type: "string", description: "DG var name on the player" },
        value: { type: "string", description: "Value the var must equal" },
      },
      required: ["kind", "var", "value"],
    },
  ],
};

const rewardSchema = {
  oneOf: [
    {
      type: "object",
      properties: {
        kind: { const: "gold" },
        amount: { type: "number", description: "Gold delivered on completion" },
      },
      required: ["kind", "amount"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "item" },
        vnum: { type: "string", description: "Item prototype vnum" },
        qty: { type: "number", description: "How many. Default 1." },
      },
      required: ["kind", "vnum"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "skill_xp" },
        skill: { type: "string", description: "Skill key (e.g. 'orcish', 'sword')" },
        amount: { type: "number", description: "XP awarded" },
      },
      required: ["kind", "skill", "amount"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "achievement" },
        key: {
          type: "string",
          description:
            "Achievement key. Granted via the listener path; not granted by hand-authored CompleteQuest dialogue effect.",
        },
      },
      required: ["kind", "key"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "learn_recipe" },
        recipe_id: { type: "string", description: "Recipe vnum (e.g. 'smith:iron_sword')" },
      },
      required: ["kind", "recipe_id"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "embrace_clan" },
        clan: {
          type: "string",
          enum: ["brujah", "toreador", "ventrue", "nosferatu", "gangrel"],
          description:
            "Acknowledges a thinblood vampire into the named clan on completion. Sire defaults to the quest's giver_mob_vnum prototype name. No-op for mortals or already-acknowledged kindred.",
        },
      },
      required: ["kind", "clan"],
    },
    {
      type: "object",
      properties: {
        kind: { const: "embrace_anarch" },
        discipline: {
          type: "string",
          description:
            "Optional hardcoded discipline (one of: potence, celerity, auspex, obfuscate, fortitude). Omit to let the reward read the player's runtime pick from the active quest's choice_vars[\"discipline\"] — set earlier in dialogue via SetQuestChoice. Lifts thinblood gates without claiming a clan; stamps the `anarch_unbound` trait and sets sire to \"Anarch Unbound\".",
        },
      },
      required: ["kind"],
    },
  ],
};

export const questToolDefinitions = [
  {
    name: "list_quests",
    description: "List all quest prototypes.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "get_quest",
    description: "Get a quest by vnum.",
    inputSchema: {
      type: "object",
      properties: {
        vnum: { type: "string", description: "Quest vnum (e.g. 'qst:100')" },
      },
      required: ["vnum"],
    },
  },
  {
    name: "create_quest",
    description:
      "Create a new quest prototype. Objectives drive auto-progression; KillMob and BringItem (with return_to_mob_vnum) auto-advance from the combat / give listeners. Rewards are granted on completion via try_complete (kill+turn-in path) or via CompleteQuest dialogue effect.",
    inputSchema: {
      type: "object",
      properties: {
        vnum: { type: "string", description: "Unique quest vnum, e.g. 'qst:100'." },
        name: { type: "string", description: "Display name." },
        keywords: {
          type: "array",
          items: { type: "string" },
          description: "Aliases for `quest <name>` resolution.",
        },
        summary: { type: "string", description: "One-line summary in the quest log." },
        description: { type: "string", description: "Long description shown on accept / detail." },
        completion_text: { type: "string", description: "Text shown on successful turn-in." },
        objectives: { type: "array", items: objectiveSchema },
        rewards: { type: "array", items: rewardSchema },
        repeatable: { type: "boolean", description: "Allow re-accept after completion." },
        giver_mob_vnum: {
          type: "string",
          description: "Canonical questgiver mob vnum (used by builder tooling).",
        },
        prereq_quest_vnum: {
          type: "string",
          description:
            "Optional vnum of a quest that must be completed before this one can be accepted. Empty/clear/none clears.",
        },
        min_player_skill_total: {
          type: "number",
          description:
            "Soft level gate: sum of player skill levels must meet this to accept. 0 clears.",
        },
        duration_secs: {
          type: "number",
          description:
            "Time limit in seconds (None / 0 = no expiry). Active instances are dropped after duration_secs since started_at.",
        },
        achievement_set_prereq: {
          type: "object",
          description:
            "Set-count gate: quest is offerable only when at least min_count of the listed achievement keys are unlocked. Pair with QuestReward::Achievement on the upstream quests to assemble 'completed N of M' endgame gates. Empty keys or non-positive min_count clears the prereq.",
          properties: {
            keys: { type: "array", items: { type: "string" } },
            min_count: { type: "number" },
          },
          required: ["keys", "min_count"],
        },
      },
      required: ["vnum", "name"],
    },
  },
  {
    name: "update_quest",
    description:
      "Update an existing quest. Any omitted field is left unchanged. Passing a new `objectives` or `rewards` array REPLACES the existing list.",
    inputSchema: {
      type: "object",
      properties: {
        vnum: { type: "string", description: "Quest vnum to update." },
        name: { type: "string" },
        keywords: { type: "array", items: { type: "string" } },
        summary: { type: "string" },
        description: { type: "string" },
        completion_text: { type: "string" },
        objectives: { type: "array", items: objectiveSchema },
        rewards: { type: "array", items: rewardSchema },
        repeatable: { type: "boolean" },
        giver_mob_vnum: { type: "string" },
        prereq_quest_vnum: { type: "string" },
        min_player_skill_total: { type: "number" },
        duration_secs: { type: "number" },
        achievement_set_prereq: {
          type: "object",
          description:
            "Set-count gate. Pass {keys: [], min_count: 0} (or any object with empty keys / non-positive min_count) to clear.",
          properties: {
            keys: { type: "array", items: { type: "string" } },
            min_count: { type: "number" },
          },
          required: ["keys", "min_count"],
        },
      },
      required: ["vnum"],
    },
  },
  {
    name: "delete_quest",
    description: "Delete a quest prototype by vnum.",
    inputSchema: {
      type: "object",
      properties: {
        vnum: { type: "string", description: "Quest vnum to delete" },
      },
      required: ["vnum"],
    },
  },
];
