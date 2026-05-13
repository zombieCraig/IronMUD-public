export const logToolDefinitions = [
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
];
//# sourceMappingURL=logs.js.map