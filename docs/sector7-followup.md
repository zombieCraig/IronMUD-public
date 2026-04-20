# Sector 7 Build -- Follow-up Items

Items that could not be completed via MCP during the initial build due to missing API support. Each section notes what was built, what's missing, and which MCP fix resolves it.

**Summary:** 7 of 8 items completed. Only elevator transport (item 8) remains deferred.

## 1. Area Reset -- DONE

**Status:** Completed. Area reset runs successfully via `POST /areas/{id}/reset`.

**What was fixed:** Aligned Rust API response format with the standard `{ success, data: { ... } }` wrapper. Updated MCP client return type.

**Result:** All 17 spawn points populated.

---

## 2. Door Key Linkage -- DONE

**Status:** Completed. Both locked doors now reference their key items.

**What was fixed:** Added `key_id: Option<String>` to `SetDoorRequest` in Rust API. Handler now parses and assigns key UUID to `DoorState`.

**Result:**
- Gang Hideout (west from undercity_junction): key_id = `5e13bce9-d3da-4d7d-aa5a-862533f9e900` (Chrome Fang Keycard)
- Smuggler's Den (south from black_market): key_id = `121a514b-18f1-44fa-b00c-b23891d1500f` (Unmarked Access Chip)

---

## 3. Room Safe Flag -- DONE

**Status:** Completed. 14 rooms set to `combat_zone: "safe"`.

**What was fixed:** Added `safe: bool` to `RoomFlagsRequest`. Handler sets `CombatZoneType::Safe` when true.

**Result:** Safe rooms: main_plaza, noodle_bar, arms_shop, tech_shop, pawn_shop, food_market, corp_lobby, corp_office, corp_lounge, corp_shopping, corp_medbay, undercity_clinic, elevator_car, elevator_lobby.

---

## 4. Mobile Healer Flag -- DONE

**Status:** Completed. Doc Vex fully configured as healer.

**What was fixed:** Added all missing flags (`healer`, `no_attack`, `cowardly`, `can_open_doors`, `leasing_agent`) to `MobileFlagsRequest`. Added healer config fields to mobile API.

**Result:**
- Doc Vex: `flags.healer = true`, `flags.no_attack = true`
- `healer_type = "medic"`, `healing_cost_multiplier = 150`

---

## 5. Firearm and Ammunition Item Fields -- DONE

**Status:** Completed. All 10 items updated with firearm/ammo/attachment properties.

**What was fixed:** Added 13 optional fields to `CreateItemRequest` and `UpdateItemRequest`: caliber, ranged_type, magazine_size, fire_mode, supported_fire_modes, noise_level, two_handed, ammo_count, ammo_damage_bonus, attachment_slot, attachment_accuracy_bonus, attachment_noise_reduction, attachment_magazine_bonus.

**Result:**

| Item | Key Fields |
|------|-----------|
| Voss LP-7 Light Pistol | 9mm, pistol, mag 12, semi, loud |
| Karga .45 Heavy Pistol | .45, pistol, mag 8, semi, loud |
| Raze-9 SMG | 9mm, smg, mag 30, auto (semi/burst/auto), loud, two-handed |
| Takamura AR-7 Assault Rifle | 5.56, rifle, mag 30, semi (semi/burst/auto), loud, two-handed |
| 9mm Magazine | 9mm, 12 rounds, +0 damage |
| .45 ACP Magazine | .45, 8 rounds, +1 damage |
| 5.56mm Magazine | 5.56, 30 rounds, +2 damage |
| Box of 9mm Rounds | 9mm, 50 rounds, +0 damage |
| Suppressor | barrel slot, noise reduction 3 |
| Laser Sight | rail slot, accuracy bonus 2 |

---

## 6. Shop Configuration -- DONE

**Status:** Completed. All three shopkeepers configured with stock and rates.

**What was fixed:** Added shop config fields to mobile API: `shop_stock`, `shop_sell_rate`, `shop_buy_rate`, `shop_buys_types`.

**Result:**

| Shopkeeper | Stock | Sell Rate | Buy Rate | Buys |
|-----------|-------|-----------|----------|------|
| Viktor (Arms) | light_pistol, heavy_pistol, smg, assault_rifle, 9mm_mag, 45_mag, 556_mag, 9mm_box, suppressor, laser_sight | 140% | 40% | weapon |
| Mama Chen (Food) | noodle_bowl, synth_skewer, synthcaf | 120% | 30% | food |
| Scratch (Black Market) | tactical_vest, heavy_pistol, stim_pack, data_chip | 175% | 60% | weapon, armor, misc |

---

## 7. Daily Routines -- DONE

**Status:** Completed. All 5 NPCs have daily routine schedules.

**What was fixed:** Added `POST /mobiles/:id/routine` and `DELETE /mobiles/:id/routine/:index` endpoints. Added `add_mobile_routine` and `remove_mobile_routine` MCP tools.

**Result:**

| NPC | Schedule |
|-----|----------|
| Viktor | 08:00 working at arms_shop (sentinel) / 20:00 offduty at noodle_bar |
| Mama Chen | 06:00 working at noodle_bar (sentinel) / 22:00 offduty at food_market |
| Scratch | 20:00 working at black_market (sentinel) / 06:00 offduty at smuggler_den |
| Corp Guard (day) | 06:00 patrolling at corp_security_post / 18:00 sleeping at corp_lounge |
| Corp Guard (night) | 18:00 patrolling at corp_security_post / 06:00 sleeping at corp_lounge |

---

## 8. Elevator Transport -- DEFERRED

**Status:** Not yet implemented. Requires transport API endpoints that don't exist.

**Affected rooms:** elevator_car, elevator_lobby, corp_lobby

**What's needed:** Transport route API endpoints for creating elevator stops and schedules. Currently only configurable via `tedit` in-game commands.

**When ready:** Create elevator connecting street level (elevator_lobby) to corporate level (corp_lobby) via elevator_car, with on-demand operation and 3s travel time.

---

## Reference: Entity UUIDs

### Key Room UUIDs
| Room | UUID |
|------|------|
| gang_hideout | 5eb6a1e2-66a1-4d6a-9fe3-78f79209cb9b |
| smuggler_den | 4a98e3dd-c30b-449d-b614-aa828fec40b7 |
| undercity_clinic | 532eaabf-fdcd-46f9-bd88-bc69a48cf6ba |
| undercity_junction | c2a84caa-d91d-424b-8697-22046bb555a3 |
| black_market | bae56c14-1e7c-4884-b58a-639c9ffd0461 |
| elevator_lobby | 18786af9-3eee-4a64-a97e-c90dfd4eecf5 |
| elevator_car | d3e52fb0-e3b0-4170-8814-f8b12e65b930 |
| corp_lobby | 0174cc40-0aa0-48bd-80ad-2e5092d2b747 |

### Key Item UUIDs
| Item | UUID |
|------|------|
| fang_key | 5e13bce9-d3da-4d7d-aa5a-862533f9e900 |
| smuggler_key | 121a514b-18f1-44fa-b00c-b23891d1500f |
| light_pistol | 79abc4f7-7de7-4201-8949-426be8b3e9bd |
| heavy_pistol | 9b757560-6bcd-42ce-b3ea-379360d8b876 |
| smg | b55dba42-64be-4689-8d22-e8499c91f2ff |
| assault_rifle | 43566ada-2bed-4b90-b156-8fe3f372bced |
| 9mm_mag | bb4de857-283f-4596-9f6e-beaa6826d4fc |
| 45_mag | cb0e1cef-7f66-4b15-87b3-53837f5f66ba |
| 556_mag | f612825e-9f18-4bf2-915b-5b215dbeab44 |
| 9mm_box | 36a48d05-d708-4b39-81a1-a863f0c67f32 |
| suppressor | 0032e560-c271-40c0-82e6-1f9e6e4f4495 |
| laser_sight | a97db3b5-44c6-43f9-9df6-ad3a0db395c0 |

### Key Mobile UUIDs
| Mobile | UUID |
|--------|------|
| doc_vex | c7387258-ffec-4705-acaf-65659697baa0 |
| arms_dealer (Viktor) | 8f361eb2-e1da-41f0-a2a2-c10b326d4759 |
| food_vendor (Mama Chen) | 7b3842ca-6612-486f-a5d0-6b3eb926bdca |
| black_market_dealer (Scratch) | 98a1a0d3-4b1e-4ee3-b822-671db01c64c4 |
| corp_guard (day) | a19890f5-8e71-4e7a-ab88-77d23fab19bd |
| corp_guard (night) | 6377d4b5-b52d-49ab-a944-ca90ed132c03 |

### Area UUID
- Sector 7: cef181f7-6a1f-41b3-ba8d-cf6a27b08910
