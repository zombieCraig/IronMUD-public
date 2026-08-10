// The content auditor.
//
// Most of this world is built through MCP, and until these existed that half of
// the building had no quality signal at all — an agent could create a room with
// no description, no exits and a copy of its neighbour's text, and nothing
// would say so, while a person doing the same in `redit` got a grade on the
// next keystroke.
//
// Findings are engine-authored: a stable `code`, a severity, and an imperative
// message naming the fix. Grades are 0-100 with a letter from one table, so
// "did that help?" is answerable by auditing again.
//
// Severities:
//   blocker  the content is broken as shipped
//   warn     it works, but a player will notice something missing
//   polish   it is fine, it could be better
//
// Polish findings are suggestions. Content that is all polish is finished
// content, and driving them to zero is not the goal.

export const auditToolDefinitions = [
  {
    name: "audit_room",
    description:
      "Grade one room and list what to fix. Checks descriptions, exits, dangling and one-way " +
      "links, duplicated text, sector and flags. Run it after creating or editing a room — " +
      "this is the check a human builder gets automatically and an agent otherwise never sees.",
    inputSchema: {
      type: "object",
      properties: {
        key: { type: "string", description: "Room vnum (e.g. 'oakvale:square') or uuid" },
      },
      required: ["key"],
    },
  },
  {
    name: "audit_item",
    description:
      "Grade one item prototype and list what to fix. Checks description, type, weapon dice, " +
      "armor value, container capacity, wear flags, value, and whether every salient noun in " +
      "the short description is addressable as a keyword.",
    inputSchema: {
      type: "object",
      properties: {
        key: { type: "string", description: "Item vnum or uuid" },
      },
      required: ["key"],
    },
  },
  {
    name: "audit_mobile",
    description:
      "Grade one mobile prototype and list what to fix. Checks keyword coverage of the short " +
      "description, long description, level, combat stats, rewards, shop setup, and whether " +
      "the mobile has anything to say or do at all.",
    inputSchema: {
      type: "object",
      properties: {
        key: { type: "string", description: "Mobile vnum or uuid" },
      },
      required: ["key"],
    },
  },
  {
    name: "audit_quest",
    description: "Grade one quest and list what to fix.",
    inputSchema: {
      type: "object",
      properties: {
        key: { type: "string", description: "Quest vnum" },
      },
      required: ["key"],
    },
  },
  {
    name: "audit_area",
    description:
      "Grade an area and everything in it: a rolled-up score, the area's own findings, and the " +
      "worst contents worth opening first. The one call to make before calling an area done — " +
      "it catches orphaned rooms, missing spawn points, absent level ranges and empty populations " +
      "that per-entity checks cannot see. A full-world read; run it deliberately, not in a loop.",
    inputSchema: {
      type: "object",
      properties: {
        key: { type: "string", description: "Area prefix, name, or uuid" },
      },
      required: ["key"],
    },
  },
  {
    name: "audit_world",
    description:
      "Grade every area in the world plus world-level checks — cross-area connectivity, bulletin " +
      "boards, postmasters, unfiled prototypes. A full-world read.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "audit_findings_by_code",
    description:
      "Every entity currently raising one finding code, across the world or inside one area. " +
      "The audit answers 'what is wrong with this thing'; this answers 'where else does this " +
      "happen' — the question to ask before deciding a check is wrong about your content. It is " +
      "also exactly the list `waive_finding` would silence one row at a time.",
    inputSchema: {
      type: "object",
      properties: {
        code: { type: "string", description: "Finding code, e.g. 'item.keywords_miss_nouns'" },
        area: { type: "string", description: "Optional area prefix to scope the search" },
      },
      required: ["code"],
    },
  },
  {
    name: "waive_finding",
    description:
      "Record a finding as reviewed and approved — a false positive. The finding leaves the " +
      "grade, every tally and the bounty board, and is reported separately as reviewed so the " +
      "suppression stays visible. Two rules: the finding must be firing right now (that is what " +
      "supplies the text the waiver is a judgement about), and a `blocker` needs an admin key. " +
      "A waiver lapses automatically if the text it was written about later changes, so it " +
      "cannot go on hiding a different problem.",
    inputSchema: {
      type: "object",
      properties: {
        code: { type: "string", description: "Finding code, e.g. 'item.keywords_miss_nouns'" },
        target: {
          type: "string",
          description: "The entity the finding is about: a vnum, an area prefix, or 'world'",
        },
        reason: {
          type: "string",
          description: "Why this is not a defect. Required — an unexplained waiver is just silencing.",
        },
      },
      required: ["code", "target", "reason"],
    },
  },
  {
    name: "list_audit_waivers",
    description:
      "Reviewed findings, world-wide or for one area. Use it to see what a grade is not showing " +
      "before trusting the grade.",
    inputSchema: {
      type: "object",
      properties: {
        area: { type: "string", description: "Optional area prefix" },
      },
    },
  },
  {
    name: "remove_audit_waiver",
    description:
      "Revoke a waiver, putting the finding back in the grade and the tallies.",
    inputSchema: {
      type: "object",
      properties: {
        code: { type: "string" },
        target: { type: "string" },
      },
      required: ["code", "target"],
    },
  },
  {
    name: "get_world_report",
    description:
      "How far along the world is: a named tier (Wilderness through World), the weighted " +
      "components behind it (size, density, depth, quality, connectivity), and what the next " +
      "rung needs. If a structural absence is capping the rating — no quests, one area, no " +
      "dialogue trees, no bulletin board — `cap_reason` says so, and that is the thing to fix " +
      "before anything else. Read from a five-minute cache, so it is cheap to ask.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "get_build_tracks",
    description:
      "Builder progress tracks for this API key's builder: a checklist of engine systems, each " +
      "with the step still to do and a hint naming the command that does it. The Builder's Path " +
      "is effectively the tutorial this engine never had — dialogue trees, DG triggers, shops, " +
      "transports, recipes, forage tables, traps, seasonal descriptions — each a system the " +
      "engine has and most worlds never use.",
    inputSchema: { type: "object", properties: {} },
  },
];
