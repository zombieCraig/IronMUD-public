import { Tool } from "@modelcontextprotocol/sdk/types.js";

export const logToolDefinitions: Tool[] = [
  {
    name: "get_builder_debug_log",
    description: "Retrieve the last 50 lines from the BUILDER DEBUG channel. Useful for troubleshooting errors and system messages.",
    inputSchema: {
      type: "object",
      properties: {
        limit: {
          type: "number",
          description: "Number of lines to retrieve (default 50, max 100)",
          minimum: 1,
          maximum: 100,
        },
      },
    },
  },
  {
    name: "list_player_sessions",
    description:
      "List recently finished player sessions (newest first): character name, start/end unix time, duration, IP, client, end reason (quit/logout/linkdead/switch), command count and unknown-command count. Requires an admin API key. Use get_player_log to read what a player typed.",
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Only this character's sessions (case-insensitive). Omit for all players.",
        },
        limit: {
          type: "number",
          description: "Max sessions to return (default 50, max 500)",
          minimum: 1,
          maximum: 500,
        },
      },
    },
  },
  {
    name: "get_player_log",
    description:
      "Read a player's activity log, oldest first: login/logout markers and every command they typed with the room vnum and outcome (ok, unknown, denied, unconscious, script_error, social, dg_suppressed). Passwords/emails are redacted; server output is not recorded. Requires an admin API key. Pass `session` (0 = latest, live sessions included) to read one whole session.",
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Character name (case-insensitive)" },
        session: {
          type: "number",
          description: "k-th most recent session (0 = latest). Overrides since/limit.",
          minimum: 0,
        },
        since: { type: "number", description: "Only entries at or after this unix time" },
        limit: {
          type: "number",
          description: "Max entries, newest kept (default 200, max 5000)",
          minimum: 1,
          maximum: 5000,
        },
      },
      required: ["name"],
    },
  },
];
