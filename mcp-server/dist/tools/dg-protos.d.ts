export declare const dgProtoToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum?: undefined;
            name?: undefined;
            kind?: undefined;
            flags?: undefined;
            body?: undefined;
            numeric_arg?: undefined;
            arglist?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum: {
                type: string;
                description: string;
            };
            name?: undefined;
            kind?: undefined;
            flags?: undefined;
            body?: undefined;
            numeric_arg?: undefined;
            arglist?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description: string;
            };
            kind: {
                type: string;
                enum: readonly ["mob", "obj", "room"];
                description: string;
            };
            flags: {
                type: string;
                description: string;
            };
            body: {
                type: string;
                description: string;
            };
            numeric_arg: {
                type: string;
                description: string;
            };
            arglist: {
                type: string;
                description: string;
            };
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description?: undefined;
            };
            kind: {
                type: string;
                enum: readonly ["mob", "obj", "room"];
                description?: undefined;
            };
            flags: {
                type: string;
                description: string;
            };
            body: {
                type: string;
                description: string;
            };
            numeric_arg: {
                type: string;
                description?: undefined;
            };
            arglist: {
                type: string;
                description?: undefined;
            };
        };
        required: string[];
    };
})[];
//# sourceMappingURL=dg-protos.d.ts.map