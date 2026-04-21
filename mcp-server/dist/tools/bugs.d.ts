export declare const bugToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            status: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            priority?: undefined;
            author?: undefined;
            message?: undefined;
            resolved_by?: undefined;
            note?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier: {
                type: string;
                description: string;
            };
            status?: undefined;
            priority?: undefined;
            author?: undefined;
            message?: undefined;
            resolved_by?: undefined;
            note?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier: {
                type: string;
                description: string;
            };
            status: {
                type: string;
                description: string;
            };
            priority: {
                type: string;
                description: string;
            };
            author?: undefined;
            message?: undefined;
            resolved_by?: undefined;
            note?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier: {
                type: string;
                description: string;
            };
            author: {
                type: string;
                description: string;
            };
            message: {
                type: string;
                description: string;
            };
            status?: undefined;
            priority?: undefined;
            resolved_by?: undefined;
            note?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier: {
                type: string;
                description: string;
            };
            resolved_by: {
                type: string;
                description: string;
            };
            note: {
                type: string;
                description: string;
            };
            status?: undefined;
            priority?: undefined;
            author?: undefined;
            message?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=bugs.d.ts.map