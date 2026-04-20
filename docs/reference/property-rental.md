# Property Rental System Reference

> **Status: Implemented**
> This document describes the property rental system as implemented.

## Overview

The property rental system allows players to rent instanced housing with unlimited storage, party access controls, and automatic rent collection. Builders create property templates that define the rooms and amenities; when players rent, they receive their own private copy of the template.

## Key Concepts

### Leasing Agent
A mobile NPC (like shopkeepers) that manages property rentals in an area. Players interact with the leasing agent to:
- List available properties
- Tour property templates
- Rent a property
- Enter their rented property
- Visit grouped players' properties

### Property Template
A builder-defined blueprint consisting of:
- One or more connected rooms (real RoomData entries)
- Pre-placed amenities (items with `no_get` flag)
- Rental price (monthly gold cost)
- Optional restrictions (max instances, level requirement)

### Lease
An active rental agreement between a player and a property. Each player can have one lease per area. The lease tracks:
- Which template was rented
- The instanced room IDs created for this player
- Rent payment status
- Access permissions for party members

### Property Instance
When a player rents a property, the template rooms are copied to create private instance rooms. These rooms:
- Are only accessible to the owner (and permitted visitors)
- Provide safe unlimited storage
- Have an "out" exit leading back to the leasing office

---

## Data Structures

### PropertyTemplate

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyTemplate {
    pub id: Uuid,
    pub vnum: String,                      // e.g., "cottage_small"
    pub name: String,                      // "Small Cottage"
    pub description: String,               // Shown when listing properties
    pub monthly_rent: i32,                 // Gold per game month
    pub entrance_room_id: Uuid,            // Template entrance room
    #[serde(default)]
    pub max_instances: i32,                // 0 = unlimited
    #[serde(default)]
    pub level_requirement: i32,            // Minimum level to rent
    #[serde(default)]
    pub area_id: Option<Uuid>,             // Which area this template belongs to
}
```

Note: Template rooms are stored as regular RoomData entries linked via `property_template_id`. Amenities are regular items placed in template rooms with `no_get` flag.

### LeaseData

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseData {
    pub id: Uuid,
    pub template_vnum: String,             // Which PropertyTemplate
    pub owner_name: String,                // Character name who rented
    pub leasing_agent_id: Uuid,            // Mobile who leased this
    pub leasing_office_room_id: Uuid,      // Room to return to via "out"
    pub area_id: Uuid,                     // Area where lease is active
    pub instanced_rooms: Vec<Uuid>,        // Actual room UUIDs created
    pub entrance_room_id: Uuid,            // Player's entrance room
    pub monthly_rent: i32,                 // Locked rent amount
    pub rent_paid_until: i64,              // Unix timestamp
    pub created_at: i64,                   // When lease started
    #[serde(default)]
    pub is_evicted: bool,                  // Ended due to non-payment
    #[serde(default)]
    pub eviction_time: Option<i64>,        // When eviction occurred
    #[serde(default)]
    pub party_access: PartyAccessLevel,    // Access for grouped players
    #[serde(default)]
    pub trusted_visitors: Vec<String>,     // Names with full access
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyAccessLevel {
    #[default]
    None,              // No party access
    VisitOnly,         // Can enter and look
    FullAccess,        // Can use amenities, take items
}
```

### EscrowData

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowData {
    pub id: Uuid,
    pub owner_name: String,                // Character who owned items
    pub items: Vec<Uuid>,                  // Item IDs held in escrow
    pub source_lease_id: Uuid,             // Original lease
    pub created_at: i64,                   // When escrow started
    pub expires_at: i64,                   // When items get deleted
    pub retrieval_fee: i32,                // Gold fee to retrieve
    #[serde(default)]
    pub destination_lease_id: Option<Uuid>, // Property to ship items to
}
```

---

## Type Modifications

### MobileFlags (src/lib.rs)

Add new flag:
```rust
#[serde(default)]
pub leasing_agent: bool,    // Can rent out property templates
```

### MobileData (src/lib.rs)

Add leasing agent fields:
```rust
// Leasing agent system (requires leasing_agent flag)
#[serde(default)]
pub property_templates: Vec<String>,     // Vnums of available PropertyTemplates
#[serde(default)]
pub leasing_area_id: Option<Uuid>,       // Area this agent manages
```

### CharacterData (src/lib.rs)

Add rental tracking fields:
```rust
// Property rental system
#[serde(default)]
pub active_leases: HashMap<Uuid, Uuid>,  // area_id -> lease_id (one per area)
#[serde(default)]
pub escrow_ids: Vec<Uuid>,               // Escrow IDs for evicted items
#[serde(default)]
pub tour_origin_room: Option<Uuid>,      // Return location after tour
#[serde(default)]
pub on_tour: bool,                       // Currently touring a template
```

### RoomData (src/lib.rs)

Add property-related fields:
```rust
// Property template fields
#[serde(default)]
pub is_property_template: bool,          // This is a template room
#[serde(default)]
pub property_template_id: Option<Uuid>,  // Which template this belongs to
#[serde(default)]
pub is_template_entrance: bool,          // Entry point for template

// Property instance fields
#[serde(default)]
pub property_lease_id: Option<Uuid>,     // Which lease owns this instance
#[serde(default)]
pub property_entrance: bool,             // Entry point for rental
```

### RoomFlags (src/lib.rs)

Add storage flag:
```rust
#[serde(default)]
pub property_storage: bool,              // Items dropped here are safe
```

### RoomExits (src/lib.rs)

Add "out" direction:
```rust
pub out: Option<Uuid>,                   // Exit from property to leasing office
```

---

## Database Trees

Add to `src/db.rs`:

```rust
property_templates: Arc<Tree>,  // PropertyTemplate storage
leases: Arc<Tree>,              // LeaseData storage
escrow: Arc<Tree>,              // EscrowData storage
```

---

## Rust Functions (src/script/property.rs)

### Property Template Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `new_property_template` | `(name: String) -> PropertyTemplate` | Create new template |
| `get_property_template` | `(id_or_vnum: String) -> PropertyTemplate \| ()` | Get by ID or vnum |
| `save_property_template` | `(template: PropertyTemplate) -> bool` | Save to database |
| `delete_property_template` | `(id: String) -> bool` | Delete template and its rooms |
| `list_property_templates` | `() -> Array` | List all templates |
| `get_template_rooms` | `(template_id: String) -> Array` | Get rooms in template |
| `count_template_instances` | `(template_vnum: String) -> i32` | Active rental count |

### Leasing Agent Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `find_leasing_agent_in_room` | `(room_id: String) -> MobileData \| ()` | Find agent |
| `add_agent_property_template` | `(mobile_id: String, vnum: String) -> bool` | Add template to agent |
| `remove_agent_property_template` | `(mobile_id: String, vnum: String) -> bool` | Remove template |
| `set_agent_leasing_area` | `(mobile_id: String, area_id: String) -> bool` | Set managed area |
| `get_agent_templates` | `(mobile_id: String) -> Array` | Get available templates |

### Lease Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `create_lease` | `(template_vnum, owner, agent_id, office_room_id) -> LeaseData \| ()` | Create and instance property |
| `get_lease` | `(lease_id: String) -> LeaseData \| ()` | Get lease by ID |
| `get_player_lease_in_area` | `(char_name: String, area_id: String) -> LeaseData \| ()` | Get player's lease in area |
| `get_all_player_leases` | `(char_name: String) -> Array` | All player's leases |
| `end_lease` | `(lease_id: String, to_escrow: bool) -> bool` | End lease, optionally escrow items |
| `is_lease_paid` | `(lease_id: String) -> bool` | Check if rent is current |
| `get_lease_days_remaining` | `(lease_id: String) -> i32` | Days until rent due |
| `pay_rent` | `(lease_id: String, months: i32) -> bool` | Manually pay rent |

### Property Access Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `set_property_access` | `(lease_id: String, level: String) -> bool` | Set party access level |
| `add_trusted_visitor` | `(lease_id: String, char_name: String) -> bool` | Add to trusted list |
| `remove_trusted_visitor` | `(lease_id: String, char_name: String) -> bool` | Remove from trusted |
| `can_enter_property` | `(lease_id: String, char_name: String) -> bool` | Check visit access |
| `can_use_property` | `(lease_id: String, char_name: String) -> bool` | Check full access |

### Property Room Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `is_property_room` | `(room_id: String) -> bool` | Check if instanced property room |
| `is_template_room` | `(room_id: String) -> bool` | Check if template room |
| `get_lease_for_room` | `(room_id: String) -> LeaseData \| ()` | Get lease for property room |
| `get_property_owner` | `(room_id: String) -> String` | Get owner name or "" |
| `get_property_entrance` | `(lease_id: String) -> String` | Get entrance room ID |

### Escrow Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `create_escrow` | `(owner, items, source_lease_id, days) -> EscrowData` | Create escrow |
| `get_player_escrow` | `(char_name: String) -> Array` | Get all player's escrow |
| `retrieve_escrow` | `(escrow_id: String, dest_room_or_lease: String) -> bool` | Retrieve items |
| `calculate_escrow_fee` | `(escrow_id: String) -> i32` | Get retrieval fee |

### Tour Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `start_tour` | `(connection_id: String, template_vnum: String) -> bool` | Begin tour |
| `end_tour` | `(connection_id: String) -> bool` | End tour, return to origin |
| `is_on_tour` | `(connection_id: String) -> bool` | Check if touring |

### Transfer Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `calculate_transfer_fee` | `(from_lease_id: String) -> i32` | Fee based on item count |
| `transfer_property` | `(from_lease_id, to_template_vnum, agent_id) -> LeaseData \| ()` | Upgrade property |

---

## Player Commands

### properties.rhai

List available property templates at the leasing office.

```
> properties

=== Riverside Realty ===

Available Properties:

  Small Cottage - 50 gold/month
    A cozy one-room cottage with basic amenities.
    Level required: 1

  Town House - 150 gold/month
    A two-story home with kitchen and storage.
    Level required: 10
    Available: 3 of 5

Use 'tour <property>' to preview, 'rent <property>' to lease.
Your gold: 500
```

**Requirements:**
- Must be in room with leasing agent
- Shows templates assigned to that agent
- Shows availability if max_instances set

### tour.rhai

Tour a property template before renting.

```
> tour cottage

You begin a tour of 'Small Cottage'...

[Teleported to template entrance]

> look
Small Cottage - Living Room
A cozy room with a fireplace and wooden floors...

> north
Small Cottage - Bedroom
A small bedroom with a simple bed...

> tour end
Tour ended. Returning to Riverside Realty.
```

**Tour Mode Restrictions:**
- Cannot pick up items
- Cannot drop items
- Cannot attack/be attacked
- Timer auto-ends after 5 minutes

### rent.rhai

Rent a property, creating an instance.

```
> rent cottage

=== Rental Agreement ===

Property: Small Cottage
Monthly Rent: 50 gold
Required Now: 50 gold (30 game days upfront)
Your Gold: 500

Confirm rental? (yes/no)

> yes

Congratulations! You have rented 'Small Cottage'.
50 gold has been deducted. Rent is paid until Day 30.
Use 'enter' to access your new home.
```

**Checks:**
- Player has enough gold
- Player doesn't already have property in this area
- Template isn't at max instances
- Player meets level requirement

**If player has existing property in area:**
```
> rent townhouse

You already have a property in this area (Small Cottage).
Would you like to upgrade? Your items can be transferred for 25 gold.
Use 'upgrade townhouse' to proceed.
```

### enter.rhai

Enter your property from the leasing office.

```
> enter

You enter your property...

[Teleported to property entrance]

Craig's Small Cottage - Living Room
A cozy room with a fireplace...
[Exits: north out]
```

**Requirements:**
- Must be in room with leasing agent for this area
- Must have active lease in this area

### visit.rhai

Visit a grouped player's property.

```
> visit alice

Alice has granted you access to their property.
You enter Alice's Small Cottage...

[Teleported to their entrance]

Alice's Small Cottage - Living Room
...
```

**Requirements:**
- Must be in room with leasing agent
- Target must be in same group
- Target must have granted access (visit or full)
- Returns to leasing office via "out"

### endlease.rhai

Voluntarily end your lease.

```
> endlease

=== End Lease ===

Property: Small Cottage
Items inside: 15

WARNING: Your items will be moved to escrow.
You will have 30 days to retrieve them for a fee.

Type 'endlease confirm' to proceed.

> endlease confirm

Your lease has been terminated.
15 items have been placed in escrow.
Use 'escrow' to view and retrieve your items.
```

### upgrade.rhai

Transfer to a new property in the same area.

```
> upgrade townhouse

=== Property Upgrade ===

Current: Small Cottage (50 gold/month)
New: Town House (150 gold/month)

Items to transfer: 15
Transfer fee: 25 gold (based on item count)
First month rent: 150 gold
Total cost: 175 gold

Your gold: 500

Confirm upgrade? (yes/no)

> yes

Upgrade complete! Your items have been transferred.
```

### escrow.rhai

View and retrieve escrowed items. Retrieval requires being at a leasing office.

Items are delivered to: same-area property (discounted fee at 10%), any other active property (full fee), or dropped at the leasing office (full fee). Container contents and potted plants are preserved through escrow.

```
> escrow

=== Your Escrow Storage ===

Escrow #1:
  Items: 15 items stored
  Retrieval Fee: 50 gold (5 gold if re-rented locally)
  Expires: 25 days remaining
  Contents: oak chest, clay plant pot, ...

Use 'escrow retrieve <number>' at a leasing office to retrieve items.

> escrow retrieve 1

You pay 5 gold and retrieve your items.
Your belongings have been delivered to your property entrance.
```

### property.rhai

Manage property access settings.

```
> property

=== Your Property ===

Name: Craig's Small Cottage
Rent: 50 gold/month
Paid until: Day 30 (15 days remaining)
Party Access: None
Trusted Visitors: (none)

> property access visit

Party access set to 'Visit Only'.
Grouped players can now enter and look around.

> property access full

Party access set to 'Full Access'.
Grouped players can now use amenities and take items.

> property trust alice

Alice added to trusted visitors (full access).

> property untrust alice

Alice removed from trusted visitors.
```

---

## Builder Commands

### pedit.rhai

Property template editor with editing mode.

#### Creating a Template

```
> pedit create cottage

Created property template 'cottage'.
You are now in the template entrance room.
Use redit to edit rooms, dig to add rooms.
Use 'pedit done' when finished.

[Teleported to new template entrance]

Template Entrance
An empty room. Use 'redit' to customize.
```

#### Editing an Existing Template

```
> pedit edit cottage

Entering template 'cottage' for editing...

[Teleported to template entrance]
```

#### Viewing Template Info (while in template)

```
> pedit

=== Property Template: cottage ===

Name: Small Cottage
Description: A cozy starter home.
Monthly Rent: 50 gold
Max Instances: 0 (unlimited)
Level Requirement: 0 (none)

Rooms: 3
Amenities: 2 (items with no_get flag)

Commands:
  pedit name <text>    - Set display name
  pedit desc           - Edit description
  pedit rent <amount>  - Set monthly rent
  pedit max <count>    - Set max instances (0=unlimited)
  pedit level <num>    - Set level requirement
  pedit entrance       - Mark current room as entrance
  pedit done           - Exit and save template
```

#### Setting Properties

```
> pedit name Cozy Cottage
Name set to 'Cozy Cottage'.

> pedit rent 75
Monthly rent set to 75 gold.

> pedit max 10
Maximum instances set to 10.

> pedit level 5
Level requirement set to 5.

> pedit entrance
Current room marked as template entrance.
```

#### Exiting Template Editing

```
> pedit done

Template 'cottage' saved.
  Rooms: 3
  Amenities: 2

Returning to Townsville Square...
```

### dig.rhai Integration

When using `dig` in a template room, the new room inherits template properties:

```
> dig north Kitchen

You create a passage to the north.

Kitchen
An empty room...
[Exits: south]

(Room automatically linked to same property template)
```

The modified `dig` command:
1. Checks if current room has `is_property_template = true`
2. If so, sets new room's `is_property_template = true`
3. Links new room to same `property_template_id`
4. Copies area_id if present

### Placing Amenities

Amenities are regular items with the `no_get` flag:

```
> ocreate stove
Created item 'stove' (#12345).

> drop stove
You drop a stove.

> oedit stove flag no_get true
no_get flag set to true.
```

When the property is rented, amenities are copied with their `no_get` flag preserved.

### plist.rhai

List all property templates.

```
> plist

=== Property Templates ===

cottage      Small Cottage        50g/mo   3 rooms   2 active
townhouse    Town House          150g/mo   5 rooms   1 active
mansion      Grand Mansion       500g/mo  10 rooms   0 active
```

### pfind.rhai

Search property templates.

```
> pfind cottage

Found 1 template matching 'cottage':

  cottage - Small Cottage
    Rent: 50 gold/month
    Rooms: 3
    Active leases: 2
```

### pdelete.rhai

Delete a property template.

```
> pdelete cottage

WARNING: This will delete the template and all its rooms.
Active leases: 2 (will NOT be affected - they have copies)

Type 'pdelete cottage confirm' to proceed.
```

### medit.rhai Integration

Configure leasing agents via mobile editor:

```
> medit agent flag leasing_agent true
leasing_agent flag set to true.

> medit agent leasing

=== Leasing Agent Configuration ===

Area: Riverside (area_123)
Templates:
  - cottage (Small Cottage)
  - townhouse (Town House)

Commands:
  medit <id> leasing area <area_id>     - Set managed area
  medit <id> leasing add <vnum>         - Add template
  medit <id> leasing remove <vnum>      - Remove template

> medit agent leasing add mansion
Added 'mansion' to available templates.
```

---

## Room Instancing Process

When `create_lease()` is called:

```rust
fn create_property_instance(
    db: &Db,
    template: &PropertyTemplate,
    owner_name: &str,
    agent_id: &Uuid,
    office_room_id: &Uuid,
    area_id: &Uuid,
) -> Result<LeaseData> {
    let now = get_unix_timestamp();
    let lease_id = Uuid::new_v4();

    // 1. Get all template rooms
    let template_rooms = db.get_rooms_by_template_id(&template.id)?;

    // 2. Create mapping: template_room_id -> new_instance_room_id
    let mut room_mapping: HashMap<Uuid, Uuid> = HashMap::new();
    let mut instanced_rooms: Vec<Uuid> = Vec::new();
    let mut entrance_room_id = None;

    // 3. First pass: Create all instance rooms
    for template_room in &template_rooms {
        let instance_id = Uuid::new_v4();

        let instance_room = RoomData {
            id: instance_id,
            title: format!("{}'s {}", owner_name, template_room.title),
            description: template_room.description.clone(),
            flags: RoomFlags {
                property_storage: true,
                safe: true,
                ..template_room.flags.clone()
            },
            // Copy seasonal descriptions
            spring_desc: template_room.spring_desc.clone(),
            summer_desc: template_room.summer_desc.clone(),
            autumn_desc: template_room.autumn_desc.clone(),
            winter_desc: template_room.winter_desc.clone(),
            // Set instance fields
            property_lease_id: Some(lease_id),
            property_entrance: template_room.is_template_entrance,
            // Clear template fields
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            // Inherit area
            area_id: Some(*area_id),
            ..Default::default()
        };

        db.save_room_data(&instance_room)?;
        room_mapping.insert(template_room.id, instance_id);
        instanced_rooms.push(instance_id);

        if template_room.is_template_entrance {
            entrance_room_id = Some(instance_id);
        }
    }

    // 4. Second pass: Reconnect exits using mapping
    for template_room in &template_rooms {
        let instance_id = room_mapping[&template_room.id];
        let mut instance_room = db.get_room_data(&instance_id)?.unwrap();

        // Map each exit to new instance room
        if let Some(target) = template_room.exits.north {
            instance_room.exits.north = room_mapping.get(&target).copied();
        }
        // ... repeat for south, east, west, up, down

        db.save_room_data(&instance_room)?;
    }

    // 5. Set entrance room's "out" exit to leasing office
    if let Some(entrance_id) = entrance_room_id {
        let mut entrance = db.get_room_data(&entrance_id)?.unwrap();
        entrance.exits.out = Some(*office_room_id);
        db.save_room_data(&entrance)?;
    }

    // 6. Copy amenity items (items with no_get flag)
    for template_room in &template_rooms {
        let items = db.get_items_in_room(&template_room.id)?;
        let instance_room_id = room_mapping[&template_room.id];

        for item in items {
            if item.flags.no_get {
                // Clone item to instance room
                let mut cloned_item = item.clone();
                cloned_item.id = Uuid::new_v4();
                cloned_item.is_prototype = false;
                db.save_item_data(&cloned_item)?;
                db.move_item_to_room(&cloned_item.id, &instance_room_id)?;
            }
        }
    }

    // 7. Calculate rent duration (configurable via rent_period_game_days setting, default 30)
    let rent_period_days = db.get_setting_or_default("rent_period_game_days", "30");
    let rent_duration = rent_period_days * SECONDS_PER_GAME_DAY;

    // 8. Create and save lease
    let lease = LeaseData {
        id: lease_id,
        template_vnum: template.vnum.clone(),
        owner_name: owner_name.to_string(),
        leasing_agent_id: *agent_id,
        leasing_office_room_id: *office_room_id,
        area_id: *area_id,
        instanced_rooms,
        entrance_room_id: entrance_room_id.unwrap(),
        monthly_rent: template.monthly_rent,
        rent_paid_until: now + rent_duration as i64,
        created_at: now,
        is_evicted: false,
        eviction_time: None,
        party_access: PartyAccessLevel::None,
        trusted_visitors: Vec::new(),
    };

    db.save_lease(&lease)?;

    Ok(lease)
}
```

---

## Background Rent Task

### Tick Configuration

```rust
// In main.rs
const RENT_TICK_INTERVAL_SECS: u64 = 300; // Check every 5 minutes

// Spawn task
tokio::spawn(async move {
    run_rent_tick(db, connections).await;
});
```

### Rent Processing

```rust
async fn run_rent_tick(db: Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(RENT_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        let now = get_unix_timestamp();
        let leases = db.list_all_leases().unwrap_or_default();

        for lease in leases {
            if lease.is_evicted {
                continue;
            }

            // Rent is due
            if now > lease.rent_paid_until {
                if let Some(mut char) = db.get_character_data(&lease.owner_name) {
                    if char.gold >= lease.monthly_rent {
                        // Auto-pay rent
                        char.gold -= lease.monthly_rent;
                        let new_paid_until = now + RENT_DURATION_SECS;

                        let mut updated = lease.clone();
                        updated.rent_paid_until = new_paid_until;

                        db.save_character_data(&char);
                        db.save_lease(&updated);

                        notify_player(&connections, &lease.owner_name,
                            &format!("Rent of {} gold deducted for your property.",
                                lease.monthly_rent));
                    } else {
                        // Eviction
                        process_eviction(&db, &connections, &lease);
                    }
                }
            }
            // Warning: 3 game days before due
            else if now > lease.rent_paid_until - (3 * SECONDS_PER_GAME_DAY) {
                let days = (lease.rent_paid_until - now) / SECONDS_PER_GAME_DAY;
                notify_player(&connections, &lease.owner_name,
                    &format!("WARNING: Property rent due in {} days!", days));
            }
        }

        // Process expired escrow
        let escrows = db.list_all_escrow().unwrap_or_default();
        for escrow in escrows {
            if now > escrow.expires_at {
                // Delete items
                for item_id in &escrow.items {
                    db.delete_item(item_id);
                }
                db.delete_escrow(&escrow.id);

                notify_player(&connections, &escrow.owner_name,
                    "Your escrowed items have expired and been deleted.");
            }
        }
    }
}
```

### Eviction Process

```rust
fn process_eviction(db: &Db, connections: &SharedConnections, lease: &LeaseData) {
    let now = get_unix_timestamp();

    // Collect player items (not amenities)
    let mut items_to_escrow = Vec::new();
    for room_id in &lease.instanced_rooms {
        let items = db.get_items_in_room(room_id).unwrap_or_default();
        for item in items {
            if !item.flags.no_get {
                items_to_escrow.push(item.id);
            }
        }
    }

    // Create escrow if items exist
    if !items_to_escrow.is_empty() {
        let escrow = EscrowData {
            id: Uuid::new_v4(),
            owner_name: lease.owner_name.clone(),
            items: items_to_escrow,
            source_lease_id: lease.id,
            created_at: now,
            expires_at: now + (30 * 24 * 60 * 60), // 30 real days
            retrieval_fee: 100 + (lease.monthly_rent / 10),
            destination_lease_id: None,
        };
        db.save_escrow(&escrow);
    }

    // Teleport any players out of property
    for room_id in &lease.instanced_rooms {
        // Move players to leasing office
        teleport_players_from_room(db, connections, room_id,
            &lease.leasing_office_room_id);
    }

    // Delete instance rooms and their contents
    for room_id in &lease.instanced_rooms {
        // Delete amenities (no_get items)
        let items = db.get_items_in_room(room_id).unwrap_or_default();
        for item in items {
            db.delete_item(&item.id);
        }
        db.delete_room(room_id);
    }

    // Mark lease as evicted
    let mut updated = lease.clone();
    updated.is_evicted = true;
    updated.eviction_time = Some(now);
    updated.instanced_rooms.clear();
    db.save_lease(&updated);

    // Notify player
    notify_player(connections, &lease.owner_name,
        "You have been EVICTED due to non-payment! Items are in escrow.");
}
```

---

## Integration Updates

### go.rhai

Handle "out" direction and property access:

```rhai
fn get_exit_for_direction(room, direction) {
    let dir = direction.to_lower();

    if dir == "out" || dir == "o" {
        return room.exits.out;
    }
    // ... existing direction handling
}

fn run_command(args, connection_id) {
    // ... existing code

    let target_room = get_room_data(exit_target);

    // Check if entering a property room
    if target_room.property_lease_id != () {
        let lease = get_lease(target_room.property_lease_id);
        if !can_enter_property(lease.id, char.name) {
            send_client_message(connection_id,
                "You don't have permission to enter this property.");
            return;
        }
    }

    // ... continue with movement
}
```

### look.rhai

Show property information:

```rhai
fn run_command(args, connection_id) {
    // ... existing code

    // Show property info
    if room.property_lease_id != () {
        let lease = get_lease(room.property_lease_id);
        output += "\n[" + lease.owner_name + "'s property]\n";
    }

    // Show "out" exit
    if room.exits.out != () {
        exits.push("out");
    }

    // ... continue
}
```

### get.rhai

Check property access for taking items:

```rhai
fn run_command(args, connection_id) {
    // ... existing code

    // Check property permissions
    if room.property_lease_id != () {
        let lease = get_lease(room.property_lease_id);
        if !can_use_property(lease.id, char.name) {
            send_client_message(connection_id,
                "You don't have permission to take items here.");
            return;
        }
    }

    // ... continue
}
```

### drop.rhai

Items in property rooms are safe (no changes needed if using `property_storage` flag to control item decay elsewhere).

---

## Implementation Phases

### Phase 1: Data Structures
1. Add PropertyTemplate, LeaseData, EscrowData to `src/lib.rs`
2. Add new fields to MobileFlags, MobileData, CharacterData, RoomData, RoomFlags, RoomExits
3. Add database trees to `src/db.rs`

### Phase 2: Core Rust Functions
1. Create `src/script/property.rs`
2. Register all property/lease/escrow functions
3. Update `src/script/mod.rs` to include new module

### Phase 3: Builder Tools
1. `pedit.rhai` - Template editor with editing mode
2. `plist.rhai`, `pfind.rhai`, `pdelete.rhai`
3. Update `medit.rhai` for leasing_agent configuration
4. Update `dig.rhai` for template room handling

### Phase 4: Player Commands
1. `properties.rhai` - List available
2. `tour.rhai` - Preview templates
3. `rent.rhai` - Create lease
4. `enter.rhai` - Access property
5. `property.rhai` - Manage settings

### Phase 5: Background Task & Integration
1. Add rent tick to `main.rs`
2. Update `go.rhai` for out direction and access control
3. Update `look.rhai` for property display
4. Update `get.rhai` for permission check
5. `visit.rhai` - Visit grouped player
6. `endlease.rhai` - Voluntary termination
7. `upgrade.rhai` - Transfer property
8. `escrow.rhai` - Retrieve items

---

## Testing Checklist

1. **Builder creates template**: `pedit create` → `redit` → `dig` → `pedit done`
2. **Builder assigns to agent**: `medit <agent> leasing add <vnum>`
3. **Player lists properties**: `properties` shows available
4. **Player tours**: `tour <name>` → walk around → `tour end`
5. **Player rents**: `rent <name>` deducts gold, creates rooms
6. **Player enters**: `enter` teleports to property
7. **Player stores items**: `drop` works, items persist
8. **Player exits**: `out` returns to leasing office
9. **Party visits**: `visit <player>` with permission
10. **Rent auto-pays**: Background task deducts gold
11. **Eviction occurs**: No gold → items to escrow → rooms deleted
12. **Escrow retrieval**: `escrow retrieve` moves items
13. **Escrow expires**: After 30 days, items deleted
14. **Upgrade works**: `upgrade` transfers items to new property
