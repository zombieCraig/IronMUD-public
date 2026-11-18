// Divine spellcraft codex tools: read the codex and each god's canon, and
// (admin keys only) moderate codex spells.

export const codexToolDefinitions = [
  {
    name: "list_codex",
    description:
      "List every codex spell (player-authored and ancient divine spells) with its god, canon status, author, veto flag, spell type and power tier.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "get_codex_spell",
    description:
      "Get one codex spell: its full spell definition plus codex metadata (origin, author, god, status, lineage, witnesses, flavor, veto).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Codex spell id (e.g. 'cx_mending_light')" },
      },
      required: ["id"],
    },
  },
  {
    name: "get_canon",
    description:
      "Get a god's canon: the spells it currently teaches (with devotion per slot), the god's display name and its influence.",
    inputSchema: {
      type: "object",
      properties: {
        god_vnum: { type: "string", description: "Vnum of the deity mobile" },
      },
      required: ["god_vnum"],
    },
  },
  {
    name: "set_codex_status",
    description:
      "Admin only. Set a codex spell's canon status: 'canon' (taught), 'faded' (known, no longer taught) or 'lost'.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Codex spell id" },
        status: { type: "string", enum: ["canon", "faded", "lost"] },
      },
      required: ["id", "status"],
    },
  },
  {
    name: "set_codex_veto",
    description:
      "Admin only. Veto a codex spell (hides it from player listings and fades it) or lift a veto. Lifting a veto leaves the spell faded; restore its status separately.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Codex spell id" },
        vetoed: { type: "boolean", description: "true to veto, false to lift the veto" },
      },
      required: ["id", "vetoed"],
    },
  },
];
