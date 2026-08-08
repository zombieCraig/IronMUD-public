export declare const bountyToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            status: {
                type: string;
                enum: string[];
                description: string;
            };
            claimed_by: {
                type: string;
                description: string;
            };
            limit: {
                type: string;
                description: string;
            };
            ticket?: undefined;
            title?: undefined;
            detail?: undefined;
            points?: undefined;
            area?: undefined;
            kind?: undefined;
            linked?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            ticket: {
                type: string;
                description: string;
            };
            status?: undefined;
            claimed_by?: undefined;
            limit?: undefined;
            title?: undefined;
            detail?: undefined;
            points?: undefined;
            area?: undefined;
            kind?: undefined;
            linked?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            title: {
                type: string;
                description: string;
            };
            detail: {
                type: string;
                description: string;
            };
            points: {
                type: string;
                description: string;
            };
            area: {
                type: string;
                description: string;
            };
            kind: {
                type: string;
                enum: string[];
                description: string;
            };
            status?: undefined;
            claimed_by?: undefined;
            limit?: undefined;
            ticket?: undefined;
            linked?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            ticket: {
                type: string;
                description: string;
            };
            linked: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            status?: undefined;
            claimed_by?: undefined;
            limit?: undefined;
            title?: undefined;
            detail?: undefined;
            points?: undefined;
            area?: undefined;
            kind?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=bounties.d.ts.map