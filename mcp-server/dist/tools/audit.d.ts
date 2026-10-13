export declare const auditToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            key: {
                type: string;
                description: string;
            };
            code?: undefined;
            area?: undefined;
            target?: undefined;
            reason?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            key?: undefined;
            code?: undefined;
            area?: undefined;
            target?: undefined;
            reason?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            code: {
                type: string;
                description: string;
            };
            area: {
                type: string;
                description: string;
            };
            key?: undefined;
            target?: undefined;
            reason?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            code: {
                type: string;
                description: string;
            };
            target: {
                type: string;
                description: string;
            };
            reason: {
                type: string;
                description: string;
            };
            key?: undefined;
            area?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            area: {
                type: string;
                description: string;
            };
            key?: undefined;
            code?: undefined;
            target?: undefined;
            reason?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            code: {
                type: string;
                description?: undefined;
            };
            target: {
                type: string;
                description?: undefined;
            };
            key?: undefined;
            area?: undefined;
            reason?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=audit.d.ts.map