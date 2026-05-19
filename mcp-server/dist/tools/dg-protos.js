// DG Scripts trigger-prototype tools — CRUD for the shared body registry
// that the builder-side `medit/oedit/redit <id> trigger dg proto …`
// commands also write into. Lets agents create / edit / inspect protos
// whose bodies are too large to paste through a telnet input buffer.
const KIND_ENUM = ["mob", "obj", "room"];
const FLAGS_HELP = "Letter-flag string mapping to trigger types. For mob: b=OnIdle, c=OnCommand, g=OnGreet, " +
    "j=OnAttack, k=OnDeath, l=OnSay, m=OnReceive, n=OnBribe, o=OnFight, p=OnHitPercent, " +
    "q=OnFlee, r=OnLoad, v=OnAlways. For item: c=OnCommand, g=OnGet, d=OnDrop, " +
    "u=OnUse, e=OnExamine, p=OnPrompt, l=OnLoad. For room: c=OnCommand, e=OnEnter, " +
    "x=OnExit, l=OnLook, p=OnPeriodic, t=OnTimeChange, w=OnWeatherChange, s=OnSeasonChange, " +
    "m=OnMonthChange.";
export const dgProtoToolDefinitions = [
    {
        name: "list_dg_protos",
        description: "List every DG trigger prototype in the registry. Returns vnum / name / kind (mob|obj|room) / flags. " +
            "Bodies are omitted from the list — use get_dg_proto for the full body.",
        inputSchema: { type: "object", properties: {} },
    },
    {
        name: "get_dg_proto",
        description: "Fetch a single DG trigger prototype by vnum, including its body, flags, and arglist.",
        inputSchema: {
            type: "object",
            properties: {
                vnum: {
                    type: "string",
                    description: "Proto vnum (free-form string, e.g. 'guard_lock_door' or the numeric vnum from a tbamud import).",
                },
            },
            required: ["vnum"],
        },
    },
    {
        name: "create_dg_proto",
        description: "Create a new DG trigger prototype in the shared registry. The body is parse-checked before save; " +
            "parse errors reject the create, non-fatal analyzer warnings come back in the response. " +
            "Use this when the body is too large to paste through the in-game OLC editor.",
        inputSchema: {
            type: "object",
            properties: {
                vnum: {
                    type: "string",
                    description: "Unique proto identifier. Free-form string (e.g. 'guard_lock_door').",
                },
                name: {
                    type: "string",
                    description: "Human-readable name shown in `trigger dg protos` listings.",
                },
                kind: {
                    type: "string",
                    enum: KIND_ENUM,
                    description: "Host entity kind the proto attaches to.",
                },
                flags: {
                    type: "string",
                    description: FLAGS_HELP,
                },
                body: {
                    type: "string",
                    description: "DG script body (multi-line). Send the whole body in one call — newlines are preserved.",
                },
                numeric_arg: {
                    type: "number",
                    description: "Numeric arg / priority used as chance% on attached triggers (or hit-percent threshold for MTRIG_HITPRCNT). Defaults to 100.",
                },
                arglist: {
                    type: "string",
                    description: "Single-line arg string (verb keyword for OnCommand triggers, speech keyword for OnSay). Optional.",
                },
            },
            required: ["vnum", "name", "kind", "flags"],
        },
    },
    {
        name: "update_dg_proto",
        description: "Update an existing DG trigger prototype. Body edits are parse-checked; on save, every live trigger " +
            "attached to this proto is refreshed in place (response includes the refreshed count). " +
            "Omit a field to leave it unchanged.",
        inputSchema: {
            type: "object",
            properties: {
                vnum: { type: "string", description: "Proto vnum to update." },
                name: { type: "string" },
                kind: { type: "string", enum: KIND_ENUM },
                flags: { type: "string", description: FLAGS_HELP },
                body: { type: "string", description: "Replacement body. Send the whole body in one call." },
                numeric_arg: { type: "number" },
                arglist: { type: "string" },
            },
            required: ["vnum"],
        },
    },
    {
        name: "delete_dg_proto",
        description: "Delete a DG trigger prototype. Attached instances are orphaned but their bodies are preserved (matches the in-game `trigger dg proto delete` behaviour).",
        inputSchema: {
            type: "object",
            properties: {
                vnum: { type: "string", description: "Proto vnum to delete." },
            },
            required: ["vnum"],
        },
    },
];
//# sourceMappingURL=dg-protos.js.map