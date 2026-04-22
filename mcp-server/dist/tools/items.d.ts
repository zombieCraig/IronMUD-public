export declare const itemToolDefinitions: ({
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
            item_type: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            weight?: undefined;
            value?: undefined;
            categories?: undefined;
            wear_location?: undefined;
            damage_dice_count?: undefined;
            damage_dice_sides?: undefined;
            damage_type?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            caliber?: undefined;
            ranged_type?: undefined;
            magazine_size?: undefined;
            fire_mode?: undefined;
            supported_fire_modes?: undefined;
            noise_level?: undefined;
            two_handed?: undefined;
            weapon_skill?: undefined;
            ammo_count?: undefined;
            ammo_damage_bonus?: undefined;
            attachment_slot?: undefined;
            attachment_accuracy_bonus?: undefined;
            attachment_noise_reduction?: undefined;
            attachment_magazine_bonus?: undefined;
            plant_prototype_vnum?: undefined;
            fertilizer_duration?: undefined;
            treats_infestation?: undefined;
            liquid_type?: undefined;
            liquid_current?: undefined;
            liquid_max?: undefined;
            liquid_effects?: undefined;
            medical_tier?: undefined;
            medical_uses?: undefined;
            treats_wound_types?: undefined;
            food_nutrition?: undefined;
            food_spoil_duration?: undefined;
            food_effects?: undefined;
            note_content?: undefined;
            id?: undefined;
            room_id?: undefined;
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
            item_type?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            weight?: undefined;
            value?: undefined;
            categories?: undefined;
            wear_location?: undefined;
            damage_dice_count?: undefined;
            damage_dice_sides?: undefined;
            damage_type?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            caliber?: undefined;
            ranged_type?: undefined;
            magazine_size?: undefined;
            fire_mode?: undefined;
            supported_fire_modes?: undefined;
            noise_level?: undefined;
            two_handed?: undefined;
            weapon_skill?: undefined;
            ammo_count?: undefined;
            ammo_damage_bonus?: undefined;
            attachment_slot?: undefined;
            attachment_accuracy_bonus?: undefined;
            attachment_noise_reduction?: undefined;
            attachment_magazine_bonus?: undefined;
            plant_prototype_vnum?: undefined;
            fertilizer_duration?: undefined;
            treats_infestation?: undefined;
            liquid_type?: undefined;
            liquid_current?: undefined;
            liquid_max?: undefined;
            liquid_effects?: undefined;
            medical_tier?: undefined;
            medical_uses?: undefined;
            treats_wound_types?: undefined;
            food_nutrition?: undefined;
            food_spoil_duration?: undefined;
            food_effects?: undefined;
            note_content?: undefined;
            id?: undefined;
            room_id?: undefined;
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
            item_type?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            weight?: undefined;
            value?: undefined;
            categories?: undefined;
            wear_location?: undefined;
            damage_dice_count?: undefined;
            damage_dice_sides?: undefined;
            damage_type?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            caliber?: undefined;
            ranged_type?: undefined;
            magazine_size?: undefined;
            fire_mode?: undefined;
            supported_fire_modes?: undefined;
            noise_level?: undefined;
            two_handed?: undefined;
            weapon_skill?: undefined;
            ammo_count?: undefined;
            ammo_damage_bonus?: undefined;
            attachment_slot?: undefined;
            attachment_accuracy_bonus?: undefined;
            attachment_noise_reduction?: undefined;
            attachment_magazine_bonus?: undefined;
            plant_prototype_vnum?: undefined;
            fertilizer_duration?: undefined;
            treats_infestation?: undefined;
            liquid_type?: undefined;
            liquid_current?: undefined;
            liquid_max?: undefined;
            liquid_effects?: undefined;
            medical_tier?: undefined;
            medical_uses?: undefined;
            treats_wound_types?: undefined;
            food_nutrition?: undefined;
            food_spoil_duration?: undefined;
            food_effects?: undefined;
            note_content?: undefined;
            id?: undefined;
            room_id?: undefined;
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
            item_type?: undefined;
            vnum_prefix?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            weight?: undefined;
            value?: undefined;
            categories?: undefined;
            wear_location?: undefined;
            damage_dice_count?: undefined;
            damage_dice_sides?: undefined;
            damage_type?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            caliber?: undefined;
            ranged_type?: undefined;
            magazine_size?: undefined;
            fire_mode?: undefined;
            supported_fire_modes?: undefined;
            noise_level?: undefined;
            two_handed?: undefined;
            weapon_skill?: undefined;
            ammo_count?: undefined;
            ammo_damage_bonus?: undefined;
            attachment_slot?: undefined;
            attachment_accuracy_bonus?: undefined;
            attachment_noise_reduction?: undefined;
            attachment_magazine_bonus?: undefined;
            plant_prototype_vnum?: undefined;
            fertilizer_duration?: undefined;
            treats_infestation?: undefined;
            liquid_type?: undefined;
            liquid_current?: undefined;
            liquid_max?: undefined;
            liquid_effects?: undefined;
            medical_tier?: undefined;
            medical_uses?: undefined;
            treats_wound_types?: undefined;
            food_nutrition?: undefined;
            food_spoil_duration?: undefined;
            food_effects?: undefined;
            note_content?: undefined;
            id?: undefined;
            room_id?: undefined;
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
            item_type: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            weight: {
                type: string;
                default: number;
            };
            value: {
                type: string;
                default: number;
            };
            categories: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            wear_location: {
                type: string;
                enum: string[];
            };
            damage_dice_count: {
                type: string;
                description: string;
            };
            damage_dice_sides: {
                type: string;
                description: string;
            };
            damage_type: {
                type: string;
                enum: string[];
            };
            armor_class: {
                type: string;
                description: string;
            };
            flags: {
                type: string;
                properties: {
                    no_drop: {
                        type: string;
                    };
                    no_get: {
                        type: string;
                    };
                    invisible: {
                        type: string;
                    };
                    glow: {
                        type: string;
                    };
                    hum: {
                        type: string;
                    };
                    plant_pot: {
                        type: string;
                    };
                    lockpick: {
                        type: string;
                        description: string;
                    };
                    is_skinned: {
                        type: string;
                        description: string;
                    };
                    boat: {
                        type: string;
                        description: string;
                    };
                    medical_tool: {
                        type: string;
                        description: string;
                    };
                };
            };
            caliber: {
                type: string;
                description: string;
            };
            ranged_type: {
                type: string;
                description: string;
            };
            magazine_size: {
                type: string;
                description: string;
            };
            fire_mode: {
                type: string;
                description: string;
            };
            supported_fire_modes: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            noise_level: {
                type: string;
                description: string;
            };
            two_handed: {
                type: string;
                description: string;
            };
            weapon_skill: {
                type: string;
                enum: string[];
                description: string;
            };
            ammo_count: {
                type: string;
                description: string;
            };
            ammo_damage_bonus: {
                type: string;
                description: string;
            };
            attachment_slot: {
                type: string;
                description: string;
            };
            attachment_accuracy_bonus: {
                type: string;
                description: string;
            };
            attachment_noise_reduction: {
                type: string;
                description: string;
            };
            attachment_magazine_bonus: {
                type: string;
                description: string;
            };
            plant_prototype_vnum: {
                type: string;
                description: string;
            };
            fertilizer_duration: {
                type: string;
                description: string;
            };
            treats_infestation: {
                type: string;
                description: string;
            };
            liquid_type: {
                type: string;
                description: string;
            };
            liquid_current: {
                type: string;
                description: string;
            };
            liquid_max: {
                type: string;
                description: string;
            };
            liquid_effects: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        effect_type: {
                            type: string;
                            description: string;
                        };
                        magnitude: {
                            type: string;
                            description: string;
                        };
                        duration: {
                            type: string;
                            description: string;
                        };
                    };
                    required: string[];
                };
            };
            medical_tier: {
                type: string;
                description: string;
            };
            medical_uses: {
                type: string;
                description: string;
            };
            treats_wound_types: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            food_nutrition: {
                type: string;
                description: string;
            };
            food_spoil_duration: {
                type: string;
                description: string;
            };
            food_effects: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        effect_type: {
                            type: string;
                            description: string;
                        };
                        magnitude: {
                            type: string;
                            description: string;
                        };
                        duration: {
                            type: string;
                            description: string;
                        };
                    };
                    required: string[];
                };
            };
            note_content: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            id?: undefined;
            room_id?: undefined;
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
            item_type: {
                type: string;
                enum: string[];
                description: string;
            };
            keywords: {
                type: string;
                items: {
                    type: string;
                };
            };
            weight: {
                type: string;
                default?: undefined;
            };
            value: {
                type: string;
                default?: undefined;
            };
            categories: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            flags: {
                type: string;
                properties?: undefined;
            };
            damage_dice_count: {
                type: string;
                description: string;
            };
            damage_dice_sides: {
                type: string;
                description: string;
            };
            damage_type: {
                type: string;
                enum: string[];
            };
            armor_class: {
                type: string;
                description: string;
            };
            wear_location: {
                type: string;
                enum: string[];
            };
            weapon_skill: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            caliber: {
                type: string;
                description?: undefined;
            };
            ranged_type: {
                type: string;
                description?: undefined;
            };
            magazine_size: {
                type: string;
                description?: undefined;
            };
            fire_mode: {
                type: string;
                description?: undefined;
            };
            supported_fire_modes: {
                type: string;
                items: {
                    type: string;
                };
                description?: undefined;
            };
            noise_level: {
                type: string;
                description?: undefined;
            };
            two_handed: {
                type: string;
                description?: undefined;
            };
            ammo_count: {
                type: string;
                description?: undefined;
            };
            ammo_damage_bonus: {
                type: string;
                description?: undefined;
            };
            attachment_slot: {
                type: string;
                description?: undefined;
            };
            attachment_accuracy_bonus: {
                type: string;
                description?: undefined;
            };
            attachment_noise_reduction: {
                type: string;
                description?: undefined;
            };
            attachment_magazine_bonus: {
                type: string;
                description?: undefined;
            };
            plant_prototype_vnum: {
                type: string;
                description: string;
            };
            fertilizer_duration: {
                type: string;
                description: string;
            };
            treats_infestation: {
                type: string;
                description: string;
            };
            liquid_type: {
                type: string;
                description: string;
            };
            liquid_current: {
                type: string;
                description: string;
            };
            liquid_max: {
                type: string;
                description: string;
            };
            liquid_effects: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        effect_type: {
                            type: string;
                            description: string;
                        };
                        magnitude: {
                            type: string;
                            description: string;
                        };
                        duration: {
                            type: string;
                            description: string;
                        };
                    };
                    required: string[];
                };
            };
            medical_tier: {
                type: string;
                description: string;
            };
            medical_uses: {
                type: string;
                description: string;
            };
            treats_wound_types: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            food_nutrition: {
                type: string;
                description: string;
            };
            food_spoil_duration: {
                type: string;
                description: string;
            };
            food_effects: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        effect_type: {
                            type: string;
                            description: string;
                        };
                        magnitude: {
                            type: string;
                            description: string;
                        };
                        duration: {
                            type: string;
                            description: string;
                        };
                    };
                    required: string[];
                };
            };
            note_content: {
                type: string;
                description: string;
            };
            limit?: undefined;
            offset?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            room_id?: undefined;
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
            item_type?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            weight?: undefined;
            value?: undefined;
            categories?: undefined;
            wear_location?: undefined;
            damage_dice_count?: undefined;
            damage_dice_sides?: undefined;
            damage_type?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            caliber?: undefined;
            ranged_type?: undefined;
            magazine_size?: undefined;
            fire_mode?: undefined;
            supported_fire_modes?: undefined;
            noise_level?: undefined;
            two_handed?: undefined;
            weapon_skill?: undefined;
            ammo_count?: undefined;
            ammo_damage_bonus?: undefined;
            attachment_slot?: undefined;
            attachment_accuracy_bonus?: undefined;
            attachment_noise_reduction?: undefined;
            attachment_magazine_bonus?: undefined;
            plant_prototype_vnum?: undefined;
            fertilizer_duration?: undefined;
            treats_infestation?: undefined;
            liquid_type?: undefined;
            liquid_current?: undefined;
            liquid_max?: undefined;
            liquid_effects?: undefined;
            medical_tier?: undefined;
            medical_uses?: undefined;
            treats_wound_types?: undefined;
            food_nutrition?: undefined;
            food_spoil_duration?: undefined;
            food_effects?: undefined;
            note_content?: undefined;
            room_id?: undefined;
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
            item_type?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            name?: undefined;
            short_desc?: undefined;
            long_desc?: undefined;
            keywords?: undefined;
            weight?: undefined;
            value?: undefined;
            categories?: undefined;
            wear_location?: undefined;
            damage_dice_count?: undefined;
            damage_dice_sides?: undefined;
            damage_type?: undefined;
            armor_class?: undefined;
            flags?: undefined;
            caliber?: undefined;
            ranged_type?: undefined;
            magazine_size?: undefined;
            fire_mode?: undefined;
            supported_fire_modes?: undefined;
            noise_level?: undefined;
            two_handed?: undefined;
            weapon_skill?: undefined;
            ammo_count?: undefined;
            ammo_damage_bonus?: undefined;
            attachment_slot?: undefined;
            attachment_accuracy_bonus?: undefined;
            attachment_noise_reduction?: undefined;
            attachment_magazine_bonus?: undefined;
            plant_prototype_vnum?: undefined;
            fertilizer_duration?: undefined;
            treats_infestation?: undefined;
            liquid_type?: undefined;
            liquid_current?: undefined;
            liquid_max?: undefined;
            liquid_effects?: undefined;
            medical_tier?: undefined;
            medical_uses?: undefined;
            treats_wound_types?: undefined;
            food_nutrition?: undefined;
            food_spoil_duration?: undefined;
            food_effects?: undefined;
            note_content?: undefined;
            id?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=items.d.ts.map