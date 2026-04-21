export declare const mobileToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            limit: {
                type: string;
                default: number;
            };
            offset: {
                type: string;
                default: number;
            };
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum_prefix: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
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
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            name: {
                type: string;
                description: string;
            };
            short_desc: {
                type: string;
                description: string;
            };
            long_desc: {
                type: string;
                description: string;
            };
            vnum: {
                type: string;
                description: string;
            };
            keywords: {
                type: string;
                items: {
                    type: string;
                };
            };
            level: {
                type: string;
                default: number;
            };
            max_hp: {
                type: string;
                default: number;
            };
            damage_dice: {
                type: string;
                description: string;
            };
            armor_class: {
                type: string;
                default: number;
            };
            flags: {
                type: string;
                properties: {
                    aggressive: {
                        type: string;
                        description: string;
                    };
                    sentinel: {
                        type: string;
                        description: string;
                    };
                    scavenger: {
                        type: string;
                        description: string;
                    };
                    shopkeeper: {
                        type: string;
                        description: string;
                    };
                    healer: {
                        type: string;
                        description: string;
                    };
                    no_attack: {
                        type: string;
                        description: string;
                    };
                    cowardly: {
                        type: string;
                        description: string;
                    };
                    can_open_doors: {
                        type: string;
                        description: string;
                    };
                    leasing_agent: {
                        type: string;
                        description: string;
                    };
                    guard: {
                        type: string;
                        description: string;
                    };
                    thief: {
                        type: string;
                        description: string;
                    };
                    cant_swim: {
                        type: string;
                        description: string;
                    };
                };
            };
            perception: {
                type: string;
                description: string;
            };
            healer_type: {
                type: string;
                description: string;
            };
            healing_free: {
                type: string;
                description: string;
            };
            healing_cost_multiplier: {
                type: string;
                description: string;
            };
            shop_sell_rate: {
                type: string;
                description: string;
            };
            shop_buy_rate: {
                type: string;
                description: string;
            };
            shop_buys_types: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            shop_stock: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            shop_preset_vnum: {
                type: string;
                description: string;
            };
            daily_routine: {
                type: string;
                items: {
                    type: string;
                    properties: {
                        start_hour: {
                            type: string;
                            description: string;
                        };
                        activity: {
                            type: string;
                            description: string;
                        };
                        destination_vnum: {
                            type: string;
                            description: string;
                        };
                        transition_message: {
                            type: string;
                            description: string;
                        };
                        suppress_wander: {
                            type: string;
                            description: string;
                        };
                        dialogue_overrides: {
                            type: string;
                            description: string;
                        };
                    };
                    required: string[];
                };
                description: string;
            };
            simulation: {
                type: string;
                description: string;
                properties: {
                    home_room_vnum: {
                        type: string;
                        description: string;
                    };
                    work_room_vnum: {
                        type: string;
                        description: string;
                    };
                    shop_room_vnum: {
                        type: string;
                        description: string;
                    };
                    preferred_food_vnum: {
                        type: string;
                        description: string;
                    };
                    work_pay: {
                        type: string;
                        description: string;
                    };
                    work_start_hour: {
                        type: string;
                        description: string;
                    };
                    work_end_hour: {
                        type: string;
                        description: string;
                    };
                    hunger_decay_rate: {
                        type: string;
                        description: string;
                    };
                    energy_decay_rate: {
                        type: string;
                        description: string;
                    };
                    comfort_decay_rate: {
                        type: string;
                        description: string;
                    };
                    low_gold_threshold: {
                        type: string;
                        description: string;
                    };
                };
                required: string[];
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            id: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description?: undefined;
            };
            short_desc: {
                type: string;
                description?: undefined;
            };
            long_desc: {
                type: string;
                description?: undefined;
            };
            vnum: {
                type: string;
                description: string;
            };
            keywords: {
                type: string;
                items: {
                    type: string;
                };
            };
            level: {
                type: string;
                default?: undefined;
            };
            max_hp: {
                type: string;
                default?: undefined;
            };
            armor_class: {
                type: string;
                default?: undefined;
            };
            gold: {
                type: string;
                description: string;
            };
            flags: {
                type: string;
                properties: {
                    aggressive: {
                        type: string;
                        description?: undefined;
                    };
                    sentinel: {
                        type: string;
                        description?: undefined;
                    };
                    scavenger: {
                        type: string;
                        description?: undefined;
                    };
                    shopkeeper: {
                        type: string;
                        description?: undefined;
                    };
                    healer: {
                        type: string;
                        description?: undefined;
                    };
                    no_attack: {
                        type: string;
                        description?: undefined;
                    };
                    cowardly: {
                        type: string;
                        description?: undefined;
                    };
                    can_open_doors: {
                        type: string;
                        description?: undefined;
                    };
                    leasing_agent: {
                        type: string;
                        description?: undefined;
                    };
                    guard: {
                        type: string;
                        description?: undefined;
                    };
                    thief: {
                        type: string;
                        description?: undefined;
                    };
                    cant_swim: {
                        type: string;
                        description?: undefined;
                    };
                };
            };
            perception: {
                type: string;
                description?: undefined;
            };
            healer_type: {
                type: string;
                description?: undefined;
            };
            healing_free: {
                type: string;
                description?: undefined;
            };
            healing_cost_multiplier: {
                type: string;
                description?: undefined;
            };
            shop_sell_rate: {
                type: string;
                description?: undefined;
            };
            shop_buy_rate: {
                type: string;
                description?: undefined;
            };
            shop_buys_types: {
                type: string;
                items: {
                    type: string;
                };
                description?: undefined;
            };
            shop_stock: {
                type: string;
                items: {
                    type: string;
                };
                description?: undefined;
            };
            shop_preset_vnum: {
                type: string;
                description?: undefined;
            };
            daily_routine: {
                type: string;
                items: {
                    type: string;
                    properties: {
                        start_hour: {
                            type: string;
                            description?: undefined;
                        };
                        activity: {
                            type: string;
                            description?: undefined;
                        };
                        destination_vnum: {
                            type: string;
                            description?: undefined;
                        };
                        transition_message: {
                            type: string;
                            description?: undefined;
                        };
                        suppress_wander: {
                            type: string;
                            description?: undefined;
                        };
                        dialogue_overrides: {
                            type: string;
                            description?: undefined;
                        };
                    };
                    required: string[];
                };
                description?: undefined;
            };
            simulation: {
                type: string;
                description: string;
                properties: {
                    home_room_vnum: {
                        type: string;
                        description?: undefined;
                    };
                    work_room_vnum: {
                        type: string;
                        description?: undefined;
                    };
                    shop_room_vnum: {
                        type: string;
                        description?: undefined;
                    };
                    preferred_food_vnum: {
                        type: string;
                        description?: undefined;
                    };
                    work_pay: {
                        type: string;
                        description?: undefined;
                    };
                    work_start_hour: {
                        type: string;
                        description?: undefined;
                    };
                    work_end_hour: {
                        type: string;
                        description?: undefined;
                    };
                    hunger_decay_rate: {
                        type: string;
                        description?: undefined;
                    };
                    energy_decay_rate: {
                        type: string;
                        description?: undefined;
                    };
                    comfort_decay_rate: {
                        type: string;
                        description?: undefined;
                    };
                    low_gold_threshold: {
                        type: string;
                        description: string;
                    };
                };
                required: string[];
            };
            remove_simulation: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            damage_dice?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            id: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            mobile_id: {
                type: string;
                description: string;
            };
            keyword: {
                type: string;
                description: string;
            };
            response: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            mobile_id: {
                type: string;
                description: string;
            };
            keyword: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            mobile_id: {
                type: string;
                description: string;
            };
            start_hour: {
                type: string;
                description: string;
            };
            activity: {
                type: string;
                description: string;
            };
            destination_vnum: {
                type: string;
                description: string;
            };
            transition_message: {
                type: string;
                description: string;
            };
            suppress_wander: {
                type: string;
                description: string;
            };
            dialogue_overrides: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            keyword?: undefined;
            response?: undefined;
            index?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            mobile_id: {
                type: string;
                description: string;
            };
            index: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
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
            room_id: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            mobile_id?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            mobile_id: {
                type: string;
                description: string;
            };
            trigger_type: {
                type: string;
                enum: string[];
                description: string;
            };
            script_name: {
                type: string;
                description: string;
            };
            enabled: {
                type: string;
                default: boolean;
            };
            interval_secs: {
                type: string;
                description: string;
            };
            chance: {
                type: string;
                description: string;
            };
            args: {
                type: string;
                items: {
                    type: string;
                };
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            level?: undefined;
            max_hp?: undefined;
            damage_dice?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            perception?: undefined;
            healer_type?: undefined;
            healing_free?: undefined;
            healing_cost_multiplier?: undefined;
            shop_sell_rate?: undefined;
            shop_buy_rate?: undefined;
            shop_buys_types?: undefined;
            shop_stock?: undefined;
            shop_preset_vnum?: undefined;
            daily_routine?: undefined;
            simulation?: undefined;
            id?: undefined;
            gold?: undefined;
            remove_simulation?: undefined;
            keyword?: undefined;
            response?: undefined;
            start_hour?: undefined;
            activity?: undefined;
            destination_vnum?: undefined;
            transition_message?: undefined;
            suppress_wander?: undefined;
            dialogue_overrides?: undefined;
            index?: undefined;
            room_id?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=mobiles.d.ts.map