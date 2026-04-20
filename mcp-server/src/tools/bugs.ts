export const bugToolDefinitions = [
  {
    name: "list_bug_reports",
    description:
      "List bug reports (only returns admin-approved reports). Use status filter to narrow results.",
    inputSchema: {
      type: "object",
      properties: {
        status: {
          type: "string",
          description:
            "Filter by status: Open, InProgress, Resolved, Closed. Omit for all approved reports.",
        },
      },
    },
  },
  {
    name: "get_bug_report",
    description:
      "Get a bug report by UUID or ticket number (only returns admin-approved reports). Accepts either a UUID or a ticket number.",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Bug report UUID or ticket number",
        },
      },
      required: ["identifier"],
    },
  },
  {
    name: "update_bug_report",
    description:
      "Update a bug report's status and/or priority. Use ticket number or UUID as identifier.",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Bug report UUID or ticket number",
        },
        status: {
          type: "string",
          description:
            "New status: Open, InProgress, Resolved, Closed",
        },
        priority: {
          type: "string",
          description: "New priority: Low, Normal, High, Critical",
        },
      },
      required: ["identifier"],
    },
  },
  {
    name: "add_bug_note",
    description: "Add an admin note to a bug report",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Bug report UUID or ticket number",
        },
        author: {
          type: "string",
          description: "Name of the note author",
        },
        message: {
          type: "string",
          description: "Note content",
        },
      },
      required: ["identifier", "author", "message"],
    },
  },
  {
    name: "close_bug_report",
    description:
      "Close a bug report with an optional resolution note",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Bug report UUID or ticket number",
        },
        resolved_by: {
          type: "string",
          description: "Name of the person resolving the bug",
        },
        note: {
          type: "string",
          description: "Optional resolution note",
        },
      },
      required: ["identifier", "resolved_by"],
    },
  },
  {
    name: "delete_bug_report",
    description: "Delete a bug report permanently",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Bug report UUID or ticket number",
        },
      },
      required: ["identifier"],
    },
  },
];
