use anyhow::Result;
use serde_json;
use sled::{Db as SledDb, Tree};
use std::path::Path;
use std::sync::Arc; // Import Arc

use crate::{
    ApiKey, AreaData, CharacterData, EscrowData, ItemData, ItemLocation, ItemType, LeaseData, MailMessage, MobileData,
    PlantInstance, PlantPrototype, PropertyTemplate, Recipe, RoomData, STARTING_ROOM_ID, ShopPreset, SpawnEntityType,
    SpawnPointData, TransportData,
};
use uuid::Uuid;

#[derive(Clone)] // Derive Clone
pub struct Db {
    db: Arc<SledDb>,       // Use Arc
    characters: Arc<Tree>, // Use Arc
    rooms: Arc<Tree>,
    vnum_index: Arc<Tree>,
    areas: Arc<Tree>,
    items: Arc<Tree>,
    mobiles: Arc<Tree>,
    spawn_points: Arc<Tree>,
    settings: Arc<Tree>,
    recipes: Arc<Tree>,
    transports: Arc<Tree>,
    // Property rental system
    property_templates: Arc<Tree>,
    leases: Arc<Tree>,
    escrow: Arc<Tree>,
    // API key system
    api_keys: Arc<Tree>,
    // Shop buy presets
    shop_presets: Arc<Tree>,
    // Mail system
    mail: Arc<Tree>,
    // Gardening system
    plants: Arc<Tree>,
    plant_prototypes: Arc<Tree>,
    // Bug reporting system
    bug_reports: Arc<Tree>,
}

/// Statistics about the world database
pub struct WorldStats {
    pub areas: usize,
    pub rooms: usize,
    pub items: usize,
    pub mobiles: usize,
    pub spawn_points: usize,
    pub recipes: usize,
    pub transports: usize,
    pub property_templates: usize,
    pub leases: usize,
    pub plant_prototypes: usize,
    pub plants: usize,
    pub characters: usize,
}

impl Db {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = sled::open(path)?;
        let characters = db.open_tree("characters")?;
        let rooms = db.open_tree("rooms")?;
        let vnum_index = db.open_tree("vnum_index")?;
        let areas = db.open_tree("areas")?;
        let items = db.open_tree("items")?;
        let mobiles = db.open_tree("mobiles")?;
        let spawn_points = db.open_tree("spawn_points")?;
        let settings = db.open_tree("settings")?;
        let recipes = db.open_tree("recipes")?;
        let transports = db.open_tree("transports")?;
        let property_templates = db.open_tree("property_templates")?;
        let leases = db.open_tree("leases")?;
        let escrow = db.open_tree("escrow")?;
        let api_keys = db.open_tree("api_keys")?;
        let shop_presets = db.open_tree("shop_presets")?;
        let mail = db.open_tree("mail")?;
        let plants = db.open_tree("plants")?;
        let plant_prototypes = db.open_tree("plant_prototypes")?;
        let bug_reports = db.open_tree("bug_reports")?;
        Ok(Self {
            db: Arc::new(db),                 // Wrap in Arc
            characters: Arc::new(characters), // Wrap in Arc
            rooms: Arc::new(rooms),
            vnum_index: Arc::new(vnum_index),
            areas: Arc::new(areas),
            items: Arc::new(items),
            mobiles: Arc::new(mobiles),
            spawn_points: Arc::new(spawn_points),
            settings: Arc::new(settings),
            recipes: Arc::new(recipes),
            transports: Arc::new(transports),
            property_templates: Arc::new(property_templates),
            leases: Arc::new(leases),
            escrow: Arc::new(escrow),
            api_keys: Arc::new(api_keys),
            shop_presets: Arc::new(shop_presets),
            mail: Arc::new(mail),
            plants: Arc::new(plants),
            plant_prototypes: Arc::new(plant_prototypes),
            bug_reports: Arc::new(bug_reports),
        })
    }

    /// Flush all pending writes to disk. Call before shutdown.
    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn get_character_data(&self, name: &str) -> Result<Option<CharacterData>> {
        // Use lowercase key for case-insensitive lookup
        let key = name.to_lowercase();
        match self.characters.get(key.as_bytes())? {
            Some(ivec) => {
                let character: CharacterData = serde_json::from_slice(&ivec)?;
                Ok(Some(character))
            }
            None => Ok(None),
        }
    }

    pub fn save_character_data(&self, character: CharacterData) -> Result<()> {
        // Use lowercase key for case-insensitive lookup, but preserve original case in data
        let key = character.name.to_lowercase();
        let value = serde_json::to_vec(&character)?;
        self.characters.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// Atomically mutate a character via CAS. See `update_mobile` for the
    /// rules — the closure may run multiple times, so keep side effects out.
    pub fn update_character<F>(&self, name: &str, mut f: F) -> Result<Option<CharacterData>>
    where
        F: FnMut(&mut CharacterData),
    {
        let key = name.to_lowercase();
        update_tree(&self.characters, key.as_bytes(), |c| f(c))
    }

    pub fn delete_character_data(&self, name: &str) -> Result<()> {
        let key = name.to_lowercase();
        self.characters.remove(key.as_bytes())?;
        Ok(())
    }

    // Hashing function
    pub fn hash_password(&self, password: &str) -> Result<String> {
        use argon2::{
            Argon2,
            password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
        };

        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Argon2 hashing error: {}", e))?
            .to_string();
        Ok(password_hash)
    }

    // Verification function
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        use argon2::Argon2;
        use argon2::password_hash::PasswordVerifier;

        let parsed_hash = argon2::password_hash::PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Argon2 parsing hash error: {}", e))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    // Room methods
    pub fn get_room_data(&self, room_id: &Uuid) -> Result<Option<RoomData>> {
        let key = room_id.as_bytes();
        match self.rooms.get(key)? {
            Some(ivec) => {
                let room: RoomData = serde_json::from_slice(&ivec)?;
                Ok(Some(room))
            }
            None => Ok(None),
        }
    }

    pub fn save_room_data(&self, room: RoomData) -> Result<()> {
        let key = room.id.as_bytes();
        let value = serde_json::to_vec(&room)?;
        self.rooms.insert(key, value)?;
        Ok(())
    }

    /// Atomically mutate a room via CAS. See `update_mobile` for the rules.
    pub fn update_room<F>(&self, room_id: &Uuid, mut f: F) -> Result<Option<RoomData>>
    where
        F: FnMut(&mut RoomData),
    {
        update_tree(&self.rooms, room_id.as_bytes(), |r| f(r))
    }

    pub fn room_exists(&self, room_id: &Uuid) -> Result<bool> {
        Ok(self.rooms.get(room_id.as_bytes())?.is_some())
    }

    /// Delete a room from the database
    pub fn delete_room(&self, room_id: &Uuid) -> Result<bool> {
        let key = room_id.as_bytes();
        Ok(self.rooms.remove(key)?.is_some())
    }

    /// List all rooms in the database
    pub fn list_all_rooms(&self) -> Result<Vec<RoomData>> {
        let mut rooms = Vec::new();
        for entry in self.rooms.iter() {
            let (_key, value) = entry?;
            let room: RoomData = serde_json::from_slice(&value)?;
            rooms.push(room);
        }
        Ok(rooms)
    }

    /// Search rooms by keyword (case-insensitive search in title and description)
    pub fn search_rooms(&self, keyword: &str) -> Result<Vec<RoomData>> {
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();
        for entry in self.rooms.iter() {
            let (_key, value) = entry?;
            let room: RoomData = serde_json::from_slice(&value)?;
            let title_match = room.title.to_lowercase().contains(&keyword_lower);
            let desc_match = room.description.to_lowercase().contains(&keyword_lower);
            if title_match || desc_match {
                results.push(room);
            }
        }
        Ok(results)
    }

    /// Set an exit on a room (used by transport system)
    /// Supports the 6 cardinal directions: north, south, east, west, up, down
    pub fn set_room_exit(&self, room_id: &Uuid, direction: &str, target_room_id: &Uuid) -> Result<()> {
        let mut room = self
            .get_room_data(room_id)?
            .ok_or_else(|| anyhow::anyhow!("Room not found: {}", room_id))?;

        let dir_lower = direction.to_lowercase();
        match dir_lower.as_str() {
            "north" | "n" => room.exits.north = Some(*target_room_id),
            "south" | "s" => room.exits.south = Some(*target_room_id),
            "east" | "e" => room.exits.east = Some(*target_room_id),
            "west" | "w" => room.exits.west = Some(*target_room_id),
            "up" | "u" => room.exits.up = Some(*target_room_id),
            "down" | "d" => room.exits.down = Some(*target_room_id),
            "out" => room.exits.out = Some(*target_room_id),
            _ => {
                // Custom exit (e.g., "elevator", "train", "portal")
                room.exits.custom.insert(dir_lower, *target_room_id);
            }
        }

        self.save_room_data(room)?;
        Ok(())
    }

    /// Clear an exit from a room (used by transport system)
    /// Supports cardinal directions, "out", and custom exits
    pub fn clear_room_exit(&self, room_id: &Uuid, direction: &str) -> Result<()> {
        let mut room = self
            .get_room_data(room_id)?
            .ok_or_else(|| anyhow::anyhow!("Room not found: {}", room_id))?;

        let dir_lower = direction.to_lowercase();
        match dir_lower.as_str() {
            "north" | "n" => room.exits.north = None,
            "south" | "s" => room.exits.south = None,
            "east" | "e" => room.exits.east = None,
            "west" | "w" => room.exits.west = None,
            "up" | "u" => room.exits.up = None,
            "down" | "d" => room.exits.down = None,
            "out" => room.exits.out = None,
            _ => {
                // Custom exit
                room.exits.custom.remove(&dir_lower);
            }
        }

        self.save_room_data(room)?;
        Ok(())
    }

    // ========== Area Functions ==========

    /// Get area data by ID
    pub fn get_area_data(&self, area_id: &Uuid) -> Result<Option<AreaData>> {
        let key = area_id.as_bytes();
        match self.areas.get(key)? {
            Some(ivec) => {
                let area: AreaData = serde_json::from_slice(&ivec)?;
                Ok(Some(area))
            }
            None => Ok(None),
        }
    }

    /// Save area data
    pub fn save_area_data(&self, area: AreaData) -> Result<()> {
        let key = area.id.as_bytes();
        let value = serde_json::to_vec(&area)?;
        self.areas.insert(key, value)?;
        Ok(())
    }

    /// Delete an area (does not delete rooms, just unassigns them)
    pub fn delete_area(&self, area_id: &Uuid) -> Result<bool> {
        // First unassign all rooms from this area
        for entry in self.rooms.iter() {
            let (key, value) = entry?;
            let mut room: RoomData = serde_json::from_slice(&value)?;
            if room.area_id == Some(*area_id) {
                room.area_id = None;
                let new_value = serde_json::to_vec(&room)?;
                self.rooms.insert(key, new_value)?;
            }
        }
        // Delete the area
        let key = area_id.as_bytes();
        Ok(self.areas.remove(key)?.is_some())
    }

    /// List all areas
    pub fn list_all_areas(&self) -> Result<Vec<AreaData>> {
        let mut areas = Vec::new();
        for entry in self.areas.iter() {
            let (_key, value) = entry?;
            let area: AreaData = serde_json::from_slice(&value)?;
            areas.push(area);
        }
        Ok(areas)
    }

    /// Get all rooms in an area
    pub fn get_rooms_in_area(&self, area_id: &Uuid) -> Result<Vec<RoomData>> {
        let mut rooms = Vec::new();
        for entry in self.rooms.iter() {
            let (_key, value) = entry?;
            let room: RoomData = serde_json::from_slice(&value)?;
            if room.area_id == Some(*area_id) {
                rooms.push(room);
            }
        }
        Ok(rooms)
    }

    /// Set the area for a room
    pub fn set_room_area(&self, room_id: &Uuid, area_id: &Uuid) -> Result<bool> {
        if let Some(mut room) = self.get_room_data(room_id)? {
            room.area_id = Some(*area_id);
            self.save_room_data(room)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clear the area from a room
    pub fn clear_room_area(&self, room_id: &Uuid) -> Result<bool> {
        if let Some(mut room) = self.get_room_data(room_id)? {
            if room.area_id.is_some() {
                room.area_id = None;
                self.save_room_data(room)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ========== Vnum Functions ==========

    /// Get room by vnum
    pub fn get_room_by_vnum(&self, vnum: &str) -> Result<Option<RoomData>> {
        let key = vnum.to_lowercase();
        if let Some(uuid_bytes) = self.vnum_index.get(key.as_bytes())? {
            let uuid_str = std::str::from_utf8(&uuid_bytes)?;
            let uuid = Uuid::parse_str(uuid_str)?;
            return self.get_room_data(&uuid);
        }
        Ok(None)
    }

    /// Set vnum for a room (updates room and index)
    pub fn set_room_vnum(&self, room_id: &Uuid, vnum: &str) -> Result<bool> {
        let vnum_lower = vnum.to_lowercase();

        // Check if vnum is already in use
        if let Some(existing_uuid_bytes) = self.vnum_index.get(vnum_lower.as_bytes())? {
            let existing_uuid_str = std::str::from_utf8(&existing_uuid_bytes)?;
            if let Ok(existing_uuid) = Uuid::parse_str(existing_uuid_str) {
                // If the existing entry points to a real room (not this one), reject
                if existing_uuid != *room_id && self.get_room_data(&existing_uuid)?.is_some() {
                    return Ok(false); // Vnum already in use by another room
                }
                // Stale entry or same room — clear it so we can re-register
                self.vnum_index.remove(vnum_lower.as_bytes())?;
            }
        }

        // Get and update room
        if let Some(mut room) = self.get_room_data(room_id)? {
            // Clear old vnum from index if exists
            if let Some(ref old_vnum) = room.vnum {
                self.vnum_index.remove(old_vnum.to_lowercase().as_bytes())?;
            }

            // Set new vnum
            room.vnum = Some(vnum_lower.clone());
            self.save_room_data(room)?;

            // Add to index
            self.vnum_index
                .insert(vnum_lower.as_bytes(), room_id.to_string().as_bytes())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clear vnum from a room
    pub fn clear_room_vnum(&self, room_id: &Uuid) -> Result<bool> {
        if let Some(mut room) = self.get_room_data(room_id)? {
            if let Some(ref vnum) = room.vnum {
                self.vnum_index.remove(vnum.to_lowercase().as_bytes())?;
                room.vnum = None;
                self.save_room_data(room)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Rebuild vnum index from room data (called on startup)
    pub fn rebuild_vnum_index(&self) -> Result<()> {
        // Clear existing index
        self.vnum_index.clear()?;

        // Rebuild from rooms
        for room in self.list_all_rooms()? {
            if let Some(ref vnum) = room.vnum {
                self.vnum_index
                    .insert(vnum.to_lowercase().as_bytes(), room.id.to_string().as_bytes())?;
            }
        }
        Ok(())
    }

    /// Migrate character keys to lowercase for case-insensitive lookup.
    /// This handles characters created before the case-insensitive change.
    pub fn migrate_character_keys_to_lowercase(&self) -> Result<()> {
        let mut migrated_count = 0;

        // Collect entries to migrate (we can't modify while iterating)
        let mut to_migrate: Vec<(Vec<u8>, CharacterData)> = Vec::new();

        for entry in self.characters.iter() {
            let (key, value) = entry?;
            let character: CharacterData = serde_json::from_slice(&value)?;
            let lowercase_key = character.name.to_lowercase();

            // Check if key is not already lowercase
            if key.as_ref() != lowercase_key.as_bytes() {
                to_migrate.push((key.to_vec(), character));
            }
        }

        // Perform migrations
        for (old_key, character) in to_migrate {
            let lowercase_key = character.name.to_lowercase();
            let value = serde_json::to_vec(&character)?;

            // Remove old mixed-case key
            self.characters.remove(&old_key)?;
            // Insert with lowercase key
            self.characters.insert(lowercase_key.as_bytes(), value)?;

            tracing::info!(
                "Migrated character '{}' key from mixed-case to lowercase",
                character.name
            );
            migrated_count += 1;
        }

        if migrated_count > 0 {
            tracing::info!("Migrated {} character key(s) to lowercase", migrated_count);
        }
        Ok(())
    }

    pub fn migrate_characters_to_valid_rooms(&self) -> Result<()> {
        let starting_room = Uuid::parse_str(STARTING_ROOM_ID)?;
        let nil_uuid = Uuid::nil();
        let mut migrated_count = 0;

        for entry in self.characters.iter() {
            let (_key, value) = entry?;
            let mut character: CharacterData = serde_json::from_slice(&value)?;

            // Check if room is nil or doesn't exist
            let needs_migration =
                character.current_room_id == nil_uuid || !self.room_exists(&character.current_room_id)?;

            if needs_migration {
                tracing::info!(
                    "Migrating character '{}' from invalid room to starting room",
                    character.name
                );
                character.current_room_id = starting_room;
                self.save_character_data(character)?;
                migrated_count += 1;
            }
        }

        if migrated_count > 0 {
            tracing::info!("Migrated {} character(s) to starting room", migrated_count);
        }
        Ok(())
    }

    // ========== Item Functions ==========

    /// Get item data by ID
    pub fn get_item_data(&self, item_id: &Uuid) -> Result<Option<ItemData>> {
        let key = item_id.as_bytes();
        match self.items.get(key)? {
            Some(ivec) => {
                let item: ItemData = serde_json::from_slice(&ivec)?;
                Ok(Some(item))
            }
            None => Ok(None),
        }
    }

    /// Save item data
    pub fn save_item_data(&self, item: ItemData) -> Result<()> {
        let key = item.id.as_bytes();
        let value = serde_json::to_vec(&item)?;
        self.items.insert(key, value)?;
        Ok(())
    }

    /// Atomically mutate an item via CAS. See `update_mobile` for the rules.
    pub fn update_item<F>(&self, item_id: &Uuid, mut f: F) -> Result<Option<ItemData>>
    where
        F: FnMut(&mut ItemData),
    {
        update_tree(&self.items, item_id.as_bytes(), |i| f(i))
    }

    /// Delete an item
    pub fn delete_item(&self, item_id: &Uuid) -> Result<bool> {
        let key = item_id.as_bytes();
        let removed = self.items.remove(key)?.is_some();
        if removed {
            // Flush to ensure deletion persists immediately
            self.db.flush()?;
        }
        Ok(removed)
    }

    /// Delete an item and recursively delete any items inside it (for containers)
    pub fn delete_item_recursive(&self, item_id: &Uuid) -> Result<bool> {
        // First delete contents if this is a container
        if let Ok(contents) = self.get_items_in_container(item_id) {
            for child in &contents {
                let _ = self.delete_item_recursive(&child.id);
            }
        }
        self.delete_item(item_id)
    }

    /// List all items in the database
    pub fn list_all_items(&self) -> Result<Vec<ItemData>> {
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            items.push(item);
        }
        Ok(items)
    }

    /// Count non-prototype items with a given vnum (for unique item enforcement)
    pub fn count_non_prototype_items_by_vnum(&self, vnum: &str) -> Result<usize> {
        let items = self.list_all_items()?;
        let count = items
            .iter()
            .filter(|i| !i.is_prototype && i.vnum.as_deref() == Some(vnum))
            .count();
        Ok(count)
    }

    pub fn count_non_prototype_mobiles_by_vnum(&self, vnum: &str) -> Result<usize> {
        Ok(self.get_mobile_instances_by_vnum(vnum)?.len())
    }

    /// Get all items in a room
    pub fn get_items_in_room(&self, room_id: &Uuid) -> Result<Vec<ItemData>> {
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let ItemLocation::Room(rid) = &item.location {
                if rid == room_id {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    /// Get all items in a character's inventory
    pub fn get_items_in_inventory(&self, char_name: &str) -> Result<Vec<ItemData>> {
        let name_lower = char_name.to_lowercase();
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let ItemLocation::Inventory(owner) = &item.location {
                if owner.to_lowercase() == name_lower {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    /// Get all items equipped by a character
    pub fn get_equipped_items(&self, char_name: &str) -> Result<Vec<ItemData>> {
        let name_lower = char_name.to_lowercase();
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let ItemLocation::Equipped(owner) = &item.location {
                if owner.to_lowercase() == name_lower {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    /// Move item to a room (gold items auto-merge with existing gold in the room)
    pub fn move_item_to_room(&self, item_id: &Uuid, room_id: &Uuid) -> Result<bool> {
        let item = match self.get_item_data(item_id)? {
            Some(i) => i,
            None => return Ok(false),
        };

        // Handle gold auto-merge
        if item.item_type == ItemType::Gold {
            if let Some(mut existing) = self.find_gold_in_room(room_id)? {
                if existing.id != *item_id {
                    // Merge into existing pile
                    existing.value += item.value;
                    crate::update_gold_descriptions(&mut existing);
                    self.save_item_data(existing)?;
                    self.delete_item(item_id)?;
                    return Ok(true);
                }
            }
        }

        // Normal item movement
        let mut item = item;
        item.location = ItemLocation::Room(*room_id);
        self.save_item_data(item)?;
        Ok(true)
    }

    /// Move item to a character's inventory
    pub fn move_item_to_inventory(&self, item_id: &Uuid, char_name: &str) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            item.location = ItemLocation::Inventory(char_name.to_lowercase());
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Move item to equipped status
    pub fn move_item_to_equipped(&self, item_id: &Uuid, char_name: &str) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            item.location = ItemLocation::Equipped(char_name.to_lowercase());
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Move item to nowhere (removes from any inventory/location)
    pub fn move_item_to_nowhere(&self, item_id: &Uuid) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            item.location = ItemLocation::Nowhere;
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get all items inside a container
    pub fn get_items_in_container(&self, container_id: &Uuid) -> Result<Vec<ItemData>> {
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let ItemLocation::Container(cid) = &item.location {
                if cid == container_id {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    /// Move item into a container (updates both container's contents list and item's location)
    pub fn move_item_to_container(&self, item_id: &Uuid, container_id: &Uuid) -> Result<bool> {
        // Get the container
        let mut container = match self.get_item_data(container_id)? {
            Some(c) if c.item_type == ItemType::Container => c,
            _ => return Ok(false),
        };

        // Get the item
        let mut item = match self.get_item_data(item_id)? {
            Some(i) => i,
            None => return Ok(false),
        };

        // Remove from old container if applicable
        if let ItemLocation::Container(old_container_id) = &item.location {
            if let Some(mut old_container) = self.get_item_data(old_container_id)? {
                old_container.container_contents.retain(|id| id != item_id);
                self.save_item_data(old_container)?;
            }
        }

        // Add to new container
        if !container.container_contents.contains(item_id) {
            container.container_contents.push(*item_id);
        }
        item.location = ItemLocation::Container(*container_id);

        self.save_item_data(container)?;
        self.save_item_data(item)?;
        Ok(true)
    }

    /// Remove item from container (updates both container's contents and item's location)
    pub fn remove_item_from_container(&self, item_id: &Uuid) -> Result<bool> {
        let mut item = match self.get_item_data(item_id)? {
            Some(i) => i,
            None => return Ok(false),
        };

        if let ItemLocation::Container(container_id) = &item.location {
            if let Some(mut container) = self.get_item_data(container_id)? {
                container.container_contents.retain(|id| id != item_id);
                self.save_item_data(container)?;
            }
            item.location = ItemLocation::Nowhere;
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ========== Mobile Inventory/Equipment Functions ==========

    /// Move item to a mobile's inventory (uses mobile UUID as owner string)
    pub fn move_item_to_mobile_inventory(&self, item_id: &Uuid, mobile_id: &Uuid) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            // Use mobile UUID as the owner identifier
            item.location = ItemLocation::Inventory(mobile_id.to_string());
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Move item to equipped on a mobile (uses mobile UUID as owner string)
    pub fn move_item_to_mobile_equipped(&self, item_id: &Uuid, mobile_id: &Uuid) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            // Use mobile UUID as the owner identifier
            item.location = ItemLocation::Equipped(mobile_id.to_string());
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get all items in a mobile's inventory
    pub fn get_items_in_mobile_inventory(&self, mobile_id: &Uuid) -> Result<Vec<ItemData>> {
        let mobile_id_str = mobile_id.to_string();
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let ItemLocation::Inventory(owner) = &item.location {
                if owner == &mobile_id_str {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    /// Get all items equipped on a mobile
    pub fn get_items_equipped_on_mobile(&self, mobile_id: &Uuid) -> Result<Vec<ItemData>> {
        let mobile_id_str = mobile_id.to_string();
        let mut items = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let ItemLocation::Equipped(owner) = &item.location {
                if owner == &mobile_id_str {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    /// Search items by keyword (case-insensitive search in name and keywords)
    pub fn search_items(&self, keyword: &str) -> Result<Vec<ItemData>> {
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            let name_match = item.name.to_lowercase().contains(&keyword_lower);
            let keyword_match = item.keywords.iter().any(|k| k.to_lowercase().contains(&keyword_lower));
            if name_match || keyword_match {
                results.push(item);
            }
        }
        Ok(results)
    }

    // ========== Gold Functions ==========

    /// Find existing gold pile in a room
    pub fn find_gold_in_room(&self, room_id: &Uuid) -> Result<Option<ItemData>> {
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if item.item_type == ItemType::Gold {
                if let ItemLocation::Room(rid) = &item.location {
                    if rid == room_id {
                        return Ok(Some(item));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Find existing gold pile in a container
    pub fn find_gold_in_container(&self, container_id: &Uuid) -> Result<Option<ItemData>> {
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if item.item_type == ItemType::Gold {
                if let ItemLocation::Container(cid) = &item.location {
                    if cid == container_id {
                        return Ok(Some(item));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Spawn gold in a room with auto-merge
    pub fn spawn_gold_in_room(&self, amount: i32, room_id: &Uuid) -> Result<ItemData> {
        // Check for existing gold pile to merge with
        if let Some(mut existing) = self.find_gold_in_room(room_id)? {
            existing.value += amount;
            crate::update_gold_descriptions(&mut existing);
            self.save_item_data(existing.clone())?;
            return Ok(existing);
        }

        // Create new gold pile
        let mut gold = crate::create_gold_item(amount);
        gold.location = ItemLocation::Room(*room_id);
        self.save_item_data(gold.clone())?;
        Ok(gold)
    }

    /// Spawn gold in a container with auto-merge
    pub fn spawn_gold_in_container(&self, amount: i32, container_id: &Uuid) -> Result<Option<ItemData>> {
        // Verify container exists and is a container
        let container = match self.get_item_data(container_id)? {
            Some(c) if c.item_type == ItemType::Container => c,
            _ => return Ok(None),
        };

        // Check for existing gold pile to merge with
        if let Some(mut existing) = self.find_gold_in_container(container_id)? {
            existing.value += amount;
            crate::update_gold_descriptions(&mut existing);
            self.save_item_data(existing.clone())?;
            return Ok(Some(existing));
        }

        // Create new gold pile
        let mut gold = crate::create_gold_item(amount);
        gold.location = ItemLocation::Container(*container_id);
        self.save_item_data(gold.clone())?;

        // Add to container contents
        let mut container = container;
        container.container_contents.push(gold.id);
        self.save_item_data(container)?;

        Ok(Some(gold))
    }

    // ========== Mobile/NPC Functions ==========

    /// Get mobile data by ID
    pub fn get_mobile_data(&self, mobile_id: &Uuid) -> Result<Option<MobileData>> {
        let key = mobile_id.as_bytes();
        match self.mobiles.get(key)? {
            Some(ivec) => {
                let mobile: MobileData = serde_json::from_slice(&ivec)?;
                Ok(Some(mobile))
            }
            None => Ok(None),
        }
    }

    /// Save mobile data
    pub fn save_mobile_data(&self, mobile: MobileData) -> Result<()> {
        let key = mobile.id.as_bytes();
        let value = serde_json::to_vec(&mobile)?;
        self.mobiles.insert(key, value)?;
        Ok(())
    }

    /// Atomically mutate a mobile via CAS. The closure receives a fresh copy
    /// from disk; if another writer committed between our read and write, we
    /// reload and re-run the closure. This is the preferred way for tick code
    /// to mutate persisted mobile state — it avoids the "load → mutate → save"
    /// race where a parallel tick's save gets silently reverted.
    ///
    /// Returns `Ok(Some(mobile))` with the post-mutation snapshot, `Ok(None)`
    /// if the mobile no longer exists.
    ///
    /// **The closure may run more than once.** Keep side effects (broadcasts,
    /// channel sends, other DB writes) outside it; the closure should only
    /// mutate the `MobileData` passed in.
    pub fn update_mobile<F>(&self, mobile_id: &Uuid, mut f: F) -> Result<Option<MobileData>>
    where
        F: FnMut(&mut MobileData),
    {
        update_tree(&self.mobiles, mobile_id.as_bytes(), |m| f(m))
    }

    /// Delete a mobile. Also releases any residency claim on a liveable room
    /// and triggers bereavement handling for every Cohabitant/Partner/Parent/
    /// Child/Sibling relation who didn't hate the deceased: happiness crash +
    /// mourning window + `bereaved_for` note. Family kinds keep their kind
    /// (a dead parent is still your parent); only Cohabitant demotes to
    /// Friend so the pair-housing pass stops targeting the dead partner.
    pub fn delete_mobile(&self, mobile_id: &Uuid) -> Result<bool> {
        // Snapshot everything we need from the dying mobile before removal.
        struct Mourner {
            id: Uuid,
            kind: crate::types::RelationshipKind,
            affinity: i32,
        }

        let (resident_vnum, deceased_name, mourners): (Option<String>, String, Vec<Mourner>) =
            match self.get_mobile_data(mobile_id) {
                Ok(Some(m)) => {
                    let mourners: Vec<Mourner> = m
                        .relationships
                        .iter()
                        .filter(|r| {
                            matches!(
                                r.kind,
                                crate::types::RelationshipKind::Partner
                                    | crate::types::RelationshipKind::Parent
                                    | crate::types::RelationshipKind::Child
                                    | crate::types::RelationshipKind::Sibling
                                    | crate::types::RelationshipKind::Cohabitant
                            )
                        })
                        .map(|r| Mourner {
                            id: r.other_id,
                            kind: r.kind,
                            affinity: r.affinity,
                        })
                        .collect();
                    (
                        m.resident_of.clone().filter(|v| !v.is_empty()),
                        m.name.clone(),
                        mourners,
                    )
                }
                _ => (None, String::new(), Vec::new()),
            };

        let key = mobile_id.as_bytes();
        let removed = self.mobiles.remove(key)?.is_some();
        if removed {
            self.db.flush()?;

            if let Some(vnum) = resident_vnum {
                if let Ok(Some(mut room)) = self.get_room_by_vnum(&vnum) {
                    let before = room.residents.len();
                    room.residents.retain(|id| id != mobile_id);
                    if room.residents.len() != before {
                        let _ = self.save_room_data(room);
                    }
                }
            }

            if !mourners.is_empty() {
                let today = self
                    .get_game_time()
                    .ok()
                    .map(|gt| crate::migration::absolute_game_day(gt.year, gt.month, gt.day) as i32)
                    .unwrap_or(0);

                for mourner in mourners {
                    // Look up the mourner's own entry back to the deceased —
                    // their stored kind + affinity are what matter for grief,
                    // not the reciprocal kind held by the deceased.
                    let (kind, affinity) = match self.get_mobile_data(&mourner.id) {
                        Ok(Some(surv)) => surv
                            .relationships
                            .iter()
                            .find(|r| r.other_id == *mobile_id)
                            .map(|r| (r.kind, r.affinity))
                            .unwrap_or((mourner.kind, mourner.affinity)),
                        _ => (mourner.kind, mourner.affinity),
                    };
                    let Some((delta, days)) = crate::social::grief_params(kind, affinity) else {
                        continue;
                    };
                    let until_day = today + days;
                    let is_cohabitant = matches!(kind, crate::types::RelationshipKind::Cohabitant);
                    let deceased_name_c = deceased_name.clone();
                    let _ = self.update_mobile(&mourner.id, |m| {
                        if let Some(s) = m.social.as_mut() {
                            s.happiness = (s.happiness + delta).clamp(0, 100);
                            // `bereaved_until_day` is the single cohabitant-style
                            // cooldown that blocks new pair bonding. Extend it to
                            // the furthest active mourning so overlapping family
                            // losses stack.
                            let new_until = match s.bereaved_until_day {
                                Some(prev) => Some(prev.max(until_day)),
                                None => Some(until_day),
                            };
                            s.bereaved_until_day = new_until;
                            s.bereaved_for.push(crate::types::BereavementNote {
                                other_id: *mobile_id,
                                other_name: deceased_name_c.clone(),
                                kind,
                                until_day,
                            });
                        }
                        if is_cohabitant {
                            if let Some(rel) = m.relationships.iter_mut().find(|r| r.other_id == *mobile_id) {
                                rel.kind = crate::types::RelationshipKind::Friend;
                            }
                        }
                        crate::social::apply_mood(m);
                    });

                    // Orphan check: `kind` is the mourner's stored kind TOWARD
                    // the deceased — so a child who just lost a parent sees
                    // `kind == Parent`. If they're juvenile and all Parent
                    // links now point at dead mobiles, flag for adoption.
                    if matches!(kind, crate::types::RelationshipKind::Parent) {
                        self.flag_orphan_if_last_parent(&mourner.id);
                    }
                }
            }
        }
        Ok(removed)
    }

    /// If the given mobile is a juvenile (Baby/Child/Adolescent) and has no
    /// living Parent remaining, flag it for the adoption pass. Called from
    /// `delete_mobile` after a parent is removed. Silent no-op on any error
    /// so bereavement cleanup never aborts.
    fn flag_orphan_if_last_parent(&self, child_id: &Uuid) {
        let Ok(Some(child)) = self.get_mobile_data(child_id) else {
            return;
        };
        let Some(chars) = child.characteristics.as_ref() else {
            return;
        };
        use crate::types::{LifeStage, life_stage_for_age};
        if !matches!(
            life_stage_for_age(chars.age),
            LifeStage::Baby | LifeStage::Child | LifeStage::Adolescent
        ) {
            return;
        }
        // Scan Parent relationships — flag if no surviving parent remains.
        let any_living_parent = child
            .relationships
            .iter()
            .filter(|r| matches!(r.kind, crate::types::RelationshipKind::Parent))
            .any(|r| {
                self.get_mobile_data(&r.other_id)
                    .ok()
                    .flatten()
                    .map(|p| p.current_hp > 0)
                    .unwrap_or(false)
            });
        if !any_living_parent {
            let _ = self.update_mobile(child_id, |m| m.adoption_pending = true);
        }
    }

    /// List all mobiles in the database
    pub fn list_all_mobiles(&self) -> Result<Vec<MobileData>> {
        let mut mobiles = Vec::new();
        for entry in self.mobiles.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Error reading mobile entry: {}", e);
                    continue;
                }
            };
            let mobile: MobileData = match serde_json::from_slice(&value) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Error deserializing mobile: {}", e);
                    continue;
                }
            };
            mobiles.push(mobile);
        }
        Ok(mobiles)
    }

    /// Get all mobiles in a room
    pub fn get_mobiles_in_room(&self, room_id: &Uuid) -> Result<Vec<MobileData>> {
        let mut mobiles = Vec::new();
        for entry in self.mobiles.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Error reading mobile entry: {}", e);
                    continue;
                }
            };
            let mobile: MobileData = match serde_json::from_slice(&value) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Error deserializing mobile: {}", e);
                    continue;
                }
            };
            if let Some(rid) = mobile.current_room_id {
                if rid == *room_id && !mobile.is_prototype {
                    mobiles.push(mobile);
                }
            }
        }
        Ok(mobiles)
    }

    /// Get mobile by vnum (prefers prototype over instance)
    pub fn get_mobile_by_vnum(&self, vnum: &str) -> Result<Option<MobileData>> {
        let vnum_lower = vnum.to_lowercase();
        let mut first_match: Option<MobileData> = None;
        for entry in self.mobiles.iter() {
            let (_key, value) = entry?;
            let mobile: MobileData = serde_json::from_slice(&value)?;
            if mobile.vnum.to_lowercase() == vnum_lower {
                if mobile.is_prototype {
                    return Ok(Some(mobile));
                }
                if first_match.is_none() {
                    first_match = Some(mobile);
                }
            }
        }
        Ok(first_match)
    }

    /// Search mobiles by keyword (case-insensitive search in name and keywords)
    pub fn search_mobiles(&self, keyword: &str) -> Result<Vec<MobileData>> {
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();
        for entry in self.mobiles.iter() {
            let (_key, value) = entry?;
            let mobile: MobileData = serde_json::from_slice(&value)?;
            let name_match = mobile.name.to_lowercase().contains(&keyword_lower);
            let keyword_match = mobile
                .keywords
                .iter()
                .any(|k| k.to_lowercase().contains(&keyword_lower));
            let vnum_match = mobile.vnum.to_lowercase().contains(&keyword_lower);
            if name_match || keyword_match || vnum_match {
                results.push(mobile);
            }
        }
        Ok(results)
    }

    /// Move mobile to a room
    pub fn move_mobile_to_room(&self, mobile_id: &Uuid, room_id: &Uuid) -> Result<bool> {
        if let Some(mut mobile) = self.get_mobile_data(mobile_id)? {
            mobile.current_room_id = Some(*room_id);
            self.save_mobile_data(mobile)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Spawn a mobile from a prototype (creates a copy with is_prototype=false)
    pub fn spawn_mobile_from_prototype(&self, vnum: &str) -> Result<Option<MobileData>> {
        if let Some(prototype) = self.get_mobile_by_vnum(vnum)? {
            let cap = prototype
                .world_max_count
                .or_else(|| if prototype.flags.unique { Some(1) } else { None });
            if let Some(max) = cap {
                let live = self.get_mobile_instances_by_vnum(vnum)?.len() as i32;
                if live >= max {
                    return Ok(None);
                }
            }
            let mut spawned = prototype.clone();
            spawned.id = Uuid::new_v4();
            spawned.is_prototype = false;
            spawned.current_hp = spawned.max_hp; // Spawn with full health
            self.save_mobile_data(spawned.clone())?;
            Ok(Some(spawned))
        } else {
            Ok(None)
        }
    }

    /// Refresh a mobile instance from its prototype
    /// Preserves: id, current_room_id, current_hp, shop_inventory
    pub fn refresh_mobile_from_prototype(&self, mobile_id: &Uuid) -> Result<Option<MobileData>> {
        let instance = match self.get_mobile_data(mobile_id)? {
            Some(m) => m,
            None => return Ok(None),
        };

        if instance.is_prototype {
            return Ok(None);
        }

        let prototype = match self.get_mobile_by_vnum(&instance.vnum)? {
            Some(p) if p.is_prototype => p,
            _ => return Ok(None),
        };

        let mut refreshed = prototype.clone();
        refreshed.id = instance.id;
        refreshed.is_prototype = false;
        refreshed.current_room_id = instance.current_room_id;
        refreshed.current_hp = instance.current_hp;
        refreshed.shop_inventory = instance.shop_inventory;

        self.save_mobile_data(refreshed.clone())?;
        Ok(Some(refreshed))
    }

    /// Get all mobile instances with a specific vnum
    pub fn get_mobile_instances_by_vnum(&self, vnum: &str) -> Result<Vec<MobileData>> {
        let vnum_lower = vnum.to_lowercase();
        let mut results = Vec::new();
        for entry in self.mobiles.iter() {
            let (_key, value) = entry?;
            let mobile: MobileData = serde_json::from_slice(&value)?;
            if !mobile.is_prototype && mobile.vnum.to_lowercase() == vnum_lower {
                results.push(mobile);
            }
        }
        Ok(results)
    }

    /// Get item by vnum (prefers prototype over instance)
    pub fn get_item_by_vnum(&self, vnum: &str) -> Result<Option<ItemData>> {
        let vnum_lower = vnum.to_lowercase();
        let mut first_match: Option<ItemData> = None;
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if let Some(ref item_vnum) = item.vnum {
                if item_vnum.to_lowercase() == vnum_lower {
                    if item.is_prototype {
                        return Ok(Some(item));
                    }
                    if first_match.is_none() {
                        first_match = Some(item);
                    }
                }
            }
        }
        Ok(first_match)
    }

    /// Spawn an item from a prototype (creates a copy with is_prototype=false)
    pub fn spawn_item_from_prototype(&self, vnum: &str) -> Result<Option<ItemData>> {
        if let Some(prototype) = self.get_item_by_vnum(vnum)? {
            if !prototype.is_prototype {
                return Ok(None); // Not a prototype
            }
            let cap = prototype
                .world_max_count
                .or_else(|| if prototype.flags.unique { Some(1) } else { None });
            if let Some(max) = cap {
                let live = self.get_item_instances_by_vnum(vnum)?.len() as i32;
                if live >= max {
                    return Ok(None);
                }
            }
            let mut spawned = prototype.clone();
            spawned.id = Uuid::new_v4();
            spawned.is_prototype = false;
            spawned.location = ItemLocation::Nowhere;
            spawned.container_contents = Vec::new();
            self.save_item_data(spawned.clone())?;
            Ok(Some(spawned))
        } else {
            Ok(None)
        }
    }

    /// Get all item instances with a specific vnum
    pub fn get_item_instances_by_vnum(&self, vnum: &str) -> Result<Vec<ItemData>> {
        let vnum_lower = vnum.to_lowercase();
        let mut results = Vec::new();
        for entry in self.items.iter() {
            let (_key, value) = entry?;
            let item: ItemData = serde_json::from_slice(&value)?;
            if !item.is_prototype {
                if let Some(ref item_vnum) = item.vnum {
                    if item_vnum.to_lowercase() == vnum_lower {
                        results.push(item);
                    }
                }
            }
        }
        Ok(results)
    }

    /// Refresh an item instance from its prototype
    /// Preserves: id, location, container_contents
    pub fn refresh_item_from_prototype(&self, item_id: &Uuid) -> Result<Option<ItemData>> {
        let instance = match self.get_item_data(item_id)? {
            Some(i) => i,
            None => return Ok(None),
        };

        if instance.is_prototype {
            return Ok(None);
        }

        let vnum = match &instance.vnum {
            Some(v) => v.clone(),
            None => return Ok(None),
        };

        let prototype = match self.get_item_by_vnum(&vnum)? {
            Some(p) if p.is_prototype => p,
            _ => return Ok(None),
        };

        let mut refreshed = prototype.clone();
        refreshed.id = instance.id;
        refreshed.is_prototype = false;
        refreshed.location = instance.location;
        refreshed.container_contents = instance.container_contents;
        // Preserve instance-specific state (not from prototype)
        refreshed.loaded_ammo = instance.loaded_ammo;
        refreshed.loaded_ammo_bonus = instance.loaded_ammo_bonus;
        // Preserve liquid fill level (instances track current amount independently)
        refreshed.liquid_current = instance.liquid_current;

        self.save_item_data(refreshed.clone())?;
        Ok(Some(refreshed))
    }

    // ========== Spawn Point Functions ==========

    /// Get spawn point by ID
    pub fn get_spawn_point(&self, spawn_point_id: &Uuid) -> Result<Option<SpawnPointData>> {
        let key = spawn_point_id.as_bytes();
        match self.spawn_points.get(key)? {
            Some(ivec) => {
                let spawn_point: SpawnPointData = serde_json::from_slice(&ivec)?;
                Ok(Some(spawn_point))
            }
            None => Ok(None),
        }
    }

    /// Save spawn point
    pub fn save_spawn_point(&self, spawn_point: SpawnPointData) -> Result<()> {
        let key = spawn_point.id.as_bytes();
        let value = serde_json::to_vec(&spawn_point)?;
        self.spawn_points.insert(key, value)?;
        Ok(())
    }

    /// Delete a spawn point
    pub fn delete_spawn_point(&self, spawn_point_id: &Uuid) -> Result<bool> {
        let key = spawn_point_id.as_bytes();
        Ok(self.spawn_points.remove(key)?.is_some())
    }

    /// List all spawn points
    pub fn list_all_spawn_points(&self) -> Result<Vec<SpawnPointData>> {
        let mut spawn_points = Vec::new();
        for entry in self.spawn_points.iter() {
            let (_key, value) = entry?;
            let sp: SpawnPointData = serde_json::from_slice(&value)?;
            spawn_points.push(sp);
        }
        Ok(spawn_points)
    }

    /// Get all spawn points for an area
    pub fn get_spawn_points_for_area(&self, area_id: &Uuid) -> Result<Vec<SpawnPointData>> {
        let mut spawn_points = Vec::new();
        for entry in self.spawn_points.iter() {
            let (_key, value) = entry?;
            let sp: SpawnPointData = serde_json::from_slice(&value)?;
            if sp.area_id == *area_id {
                spawn_points.push(sp);
            }
        }
        Ok(spawn_points)
    }

    /// Get spawn points for a specific room
    pub fn get_spawn_points_for_room(&self, room_id: &Uuid) -> Result<Vec<SpawnPointData>> {
        let mut spawn_points = Vec::new();
        for entry in self.spawn_points.iter() {
            let (_key, value) = entry?;
            let sp: SpawnPointData = serde_json::from_slice(&value)?;
            if sp.room_id == *room_id {
                spawn_points.push(sp);
            }
        }
        Ok(spawn_points)
    }

    /// Count active spawned entities for a spawn point (validates they still exist)
    pub fn count_active_spawns(&self, spawn_point: &SpawnPointData) -> Result<i32> {
        let mut count = 0;
        for entity_id in &spawn_point.spawned_entities {
            let exists = match spawn_point.entity_type {
                SpawnEntityType::Mobile => self.get_mobile_data(entity_id)?.is_some(),
                SpawnEntityType::Item => self.get_item_data(entity_id)?.is_some(),
            };
            if exists {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Clean up references to deleted entities from spawn point
    pub fn cleanup_spawn_point_refs(&self, spawn_point_id: &Uuid) -> Result<()> {
        if let Some(mut sp) = self.get_spawn_point(spawn_point_id)? {
            let mut valid_entities = Vec::new();
            for entity_id in &sp.spawned_entities {
                let exists = match sp.entity_type {
                    SpawnEntityType::Mobile => self.get_mobile_data(entity_id)?.is_some(),
                    SpawnEntityType::Item => self.get_item_data(entity_id)?.is_some(),
                };
                if exists {
                    valid_entities.push(*entity_id);
                }
            }
            sp.spawned_entities = valid_entities;
            self.save_spawn_point(sp)?;
        }
        Ok(())
    }

    // ========== Settings Functions ==========

    /// Get a setting value by key
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .settings
            .get(key.as_bytes())?
            .map(|ivec| String::from_utf8_lossy(&ivec).to_string()))
    }

    /// Set a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.settings.insert(key.as_bytes(), value.as_bytes())?;
        Ok(())
    }

    /// Delete a setting
    pub fn delete_setting(&self, key: &str) -> Result<bool> {
        Ok(self.settings.remove(key.as_bytes())?.is_some())
    }

    /// List all settings as (key, value) pairs
    pub fn list_all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut settings = Vec::new();
        for entry in self.settings.iter() {
            let (key, value) = entry?;
            settings.push((
                String::from_utf8_lossy(&key).to_string(),
                String::from_utf8_lossy(&value).to_string(),
            ));
        }
        Ok(settings)
    }

    /// Get setting with default value if not set
    pub fn get_setting_or_default(&self, key: &str, default: &str) -> Result<String> {
        Ok(self.get_setting(key)?.unwrap_or_else(|| default.to_string()))
    }

    // ========== Game Time Functions ==========

    /// Get the current game time, or create a default if not set
    pub fn get_game_time(&self) -> Result<crate::GameTime> {
        match self.get_setting("game_time")? {
            Some(json) => {
                let game_time: crate::GameTime = serde_json::from_str(&json)?;
                Ok(game_time)
            }
            None => {
                let game_time = crate::GameTime::default();
                self.save_game_time(&game_time)?;
                Ok(game_time)
            }
        }
    }

    /// Save the current game time to the database
    pub fn save_game_time(&self, game_time: &crate::GameTime) -> Result<()> {
        let json = serde_json::to_string(game_time)?;
        self.set_setting("game_time", &json)
    }

    // ========== Character Listing Functions ==========

    /// Count total number of characters in database
    pub fn count_characters(&self) -> Result<usize> {
        Ok(self.characters.len())
    }

    /// List all characters (for admin utility)
    pub fn list_all_characters(&self) -> Result<Vec<CharacterData>> {
        let mut characters = Vec::new();
        for entry in self.characters.iter() {
            let (_key, value) = entry?;
            let char: CharacterData = serde_json::from_slice(&value)?;
            characters.push(char);
        }
        Ok(characters)
    }

    /// Get names of all characters currently in combat
    pub fn get_all_characters_in_combat(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in self.characters.iter() {
            let (_key, value) = entry?;
            let char: CharacterData = serde_json::from_slice(&value)?;
            if char.combat.in_combat {
                names.push(char.name);
            }
        }
        Ok(names)
    }

    /// Get IDs of all mobiles currently in combat
    pub fn get_all_mobiles_in_combat(&self) -> Result<Vec<Uuid>> {
        tracing::debug!("get_all_mobiles_in_combat: starting iteration");
        let mut ids = Vec::new();
        let mut count = 0;
        for entry in self.mobiles.iter() {
            count += 1;
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Error reading mobile entry: {}", e);
                    continue;
                }
            };
            let mobile: MobileData = match serde_json::from_slice(&value) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Error deserializing mobile: {}", e);
                    continue;
                }
            };
            // Log non-prototype mobiles and their combat state
            if !mobile.is_prototype {
                tracing::debug!(
                    "get_all_mobiles_in_combat: checking {} ({}) - in_combat={}, targets={}",
                    mobile.name,
                    mobile.id,
                    mobile.combat.in_combat,
                    mobile.combat.targets.len()
                );
            }
            if mobile.combat.in_combat {
                ids.push(mobile.id);
            }
        }
        tracing::debug!(
            "get_all_mobiles_in_combat: iterated {} entries, {} in combat",
            count,
            ids.len()
        );
        Ok(ids)
    }

    // ========== Recipe Functions ==========

    /// Get recipe by vnum (id)
    pub fn get_recipe(&self, vnum: &str) -> Result<Option<Recipe>> {
        let key = vnum.to_lowercase();
        match self.recipes.get(key.as_bytes())? {
            Some(ivec) => {
                let recipe: Recipe = serde_json::from_slice(&ivec)?;
                Ok(Some(recipe))
            }
            None => Ok(None),
        }
    }

    /// Save recipe data
    pub fn save_recipe(&self, recipe: Recipe) -> Result<()> {
        let key = recipe.id.to_lowercase();
        let value = serde_json::to_vec(&recipe)?;
        self.recipes.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// Delete a recipe by vnum
    pub fn delete_recipe(&self, vnum: &str) -> Result<bool> {
        let key = vnum.to_lowercase();
        Ok(self.recipes.remove(key.as_bytes())?.is_some())
    }

    /// List all recipes in the database
    pub fn list_all_recipes(&self) -> Result<Vec<Recipe>> {
        let mut recipes = Vec::new();
        for entry in self.recipes.iter() {
            let (_key, value) = entry?;
            let recipe: Recipe = serde_json::from_slice(&value)?;
            recipes.push(recipe);
        }
        Ok(recipes)
    }

    /// Search recipes by keyword (case-insensitive search in name, id, and output_vnum)
    pub fn search_recipes(&self, keyword: &str) -> Result<Vec<Recipe>> {
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();
        for entry in self.recipes.iter() {
            let (_key, value) = entry?;
            let recipe: Recipe = serde_json::from_slice(&value)?;
            let id_match = recipe.id.to_lowercase().contains(&keyword_lower);
            let name_match = recipe.name.to_lowercase().contains(&keyword_lower);
            let output_match = recipe.output_vnum.to_lowercase().contains(&keyword_lower);
            if id_match || name_match || output_match {
                results.push(recipe);
            }
        }
        Ok(results)
    }

    /// Get all recipes for a specific skill
    pub fn get_recipes_by_skill(&self, skill: &str) -> Result<Vec<Recipe>> {
        let skill_lower = skill.to_lowercase();
        let mut recipes = Vec::new();
        for entry in self.recipes.iter() {
            let (_key, value) = entry?;
            let recipe: Recipe = serde_json::from_slice(&value)?;
            if recipe.skill.to_lowercase() == skill_lower {
                recipes.push(recipe);
            }
        }
        Ok(recipes)
    }

    /// Check if recipes tree is empty (for seeding)
    pub fn recipes_empty(&self) -> Result<bool> {
        Ok(self.recipes.is_empty())
    }

    /// Seed recipes from a list (used when loading from JSON on first run)
    pub fn seed_recipes(&self, recipes: Vec<Recipe>) -> Result<()> {
        for recipe in recipes {
            self.save_recipe(recipe)?;
        }
        Ok(())
    }

    // ========== Transport Functions ==========

    /// Get transport by UUID
    pub fn get_transport(&self, id: Uuid) -> Result<Option<TransportData>> {
        match self.transports.get(id.as_bytes())? {
            Some(ivec) => {
                let transport: TransportData = serde_json::from_slice(&ivec)?;
                Ok(Some(transport))
            }
            None => Ok(None),
        }
    }

    /// Get transport by vnum
    pub fn get_transport_by_vnum(&self, vnum: &str) -> Result<Option<TransportData>> {
        let vnum_lower = vnum.to_lowercase();
        for entry in self.transports.iter() {
            let (_key, value) = entry?;
            let transport: TransportData = serde_json::from_slice(&value)?;
            if let Some(ref t_vnum) = transport.vnum {
                if t_vnum.to_lowercase() == vnum_lower {
                    return Ok(Some(transport));
                }
            }
        }
        Ok(None)
    }

    /// Save transport data
    pub fn save_transport(&self, transport: &TransportData) -> Result<()> {
        let value = serde_json::to_vec(transport)?;
        self.transports.insert(transport.id.as_bytes(), value)?;
        Ok(())
    }

    /// Delete a transport by UUID
    pub fn delete_transport(&self, id: Uuid) -> Result<bool> {
        Ok(self.transports.remove(id.as_bytes())?.is_some())
    }

    /// List all transports in the database
    pub fn list_all_transports(&self) -> Result<Vec<TransportData>> {
        let mut transports = Vec::new();
        for entry in self.transports.iter() {
            let (_key, value) = entry?;
            let transport: TransportData = serde_json::from_slice(&value)?;
            transports.push(transport);
        }
        Ok(transports)
    }

    /// Search transports by keyword (case-insensitive search in name and vnum)
    pub fn search_transports(&self, keyword: &str) -> Result<Vec<TransportData>> {
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();
        for entry in self.transports.iter() {
            let (_key, value) = entry?;
            let transport: TransportData = serde_json::from_slice(&value)?;
            let name_match = transport.name.to_lowercase().contains(&keyword_lower);
            let vnum_match = transport
                .vnum
                .as_ref()
                .map(|v| v.to_lowercase().contains(&keyword_lower))
                .unwrap_or(false);
            if name_match || vnum_match {
                results.push(transport);
            }
        }
        Ok(results)
    }

    /// Get transport by interior room ID (to find what transport a room belongs to)
    pub fn get_transport_by_interior_room(&self, room_id: Uuid) -> Result<Option<TransportData>> {
        for entry in self.transports.iter() {
            let (_key, value) = entry?;
            let transport: TransportData = serde_json::from_slice(&value)?;
            if transport.interior_room_id == room_id {
                return Ok(Some(transport));
            }
        }
        Ok(None)
    }

    /// Get transports that have a stop at a specific room
    pub fn get_transports_with_stop_at(&self, room_id: Uuid) -> Result<Vec<TransportData>> {
        let mut results = Vec::new();
        for entry in self.transports.iter() {
            let (_key, value) = entry?;
            let transport: TransportData = serde_json::from_slice(&value)?;
            if transport.stops.iter().any(|s| s.room_id == room_id) {
                results.push(transport);
            }
        }
        Ok(results)
    }

    /// Check if transports tree is empty
    pub fn transports_empty(&self) -> Result<bool> {
        Ok(self.transports.is_empty())
    }

    // ========== Property Template Functions ==========

    /// Get property template by ID
    pub fn get_property_template(&self, id: &Uuid) -> Result<Option<PropertyTemplate>> {
        let key = id.as_bytes();
        match self.property_templates.get(key)? {
            Some(ivec) => {
                let template: PropertyTemplate = serde_json::from_slice(&ivec)?;
                Ok(Some(template))
            }
            None => Ok(None),
        }
    }

    /// Get property template by vnum
    pub fn get_property_template_by_vnum(&self, vnum: &str) -> Result<Option<PropertyTemplate>> {
        let vnum_lower = vnum.to_lowercase();
        for entry in self.property_templates.iter() {
            let (_key, value) = entry?;
            let template: PropertyTemplate = serde_json::from_slice(&value)?;
            if template.vnum.to_lowercase() == vnum_lower {
                return Ok(Some(template));
            }
        }
        Ok(None)
    }

    /// Save property template
    pub fn save_property_template(&self, template: &PropertyTemplate) -> Result<()> {
        let key = template.id.as_bytes();
        let value = serde_json::to_vec(template)?;
        self.property_templates.insert(key, value)?;
        Ok(())
    }

    /// Delete a property template
    pub fn delete_property_template(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.property_templates.remove(key)?.is_some())
    }

    /// List all property templates
    pub fn list_all_property_templates(&self) -> Result<Vec<PropertyTemplate>> {
        let mut templates = Vec::new();
        for entry in self.property_templates.iter() {
            let (_key, value) = entry?;
            let template: PropertyTemplate = serde_json::from_slice(&value)?;
            templates.push(template);
        }
        Ok(templates)
    }

    /// Get rooms belonging to a property template
    pub fn get_rooms_by_template_id(&self, template_id: &Uuid) -> Result<Vec<RoomData>> {
        let mut rooms = Vec::new();
        for entry in self.rooms.iter() {
            let (_key, value) = entry?;
            let room: RoomData = serde_json::from_slice(&value)?;
            if room.property_template_id == Some(*template_id) {
                rooms.push(room);
            }
        }
        Ok(rooms)
    }

    // ========== Shop Preset Functions ==========

    /// Get shop preset by ID
    pub fn get_shop_preset(&self, id: &Uuid) -> Result<Option<ShopPreset>> {
        let key = id.as_bytes();
        match self.shop_presets.get(key)? {
            Some(ivec) => {
                let preset: ShopPreset = serde_json::from_slice(&ivec)?;
                Ok(Some(preset))
            }
            None => Ok(None),
        }
    }

    /// Get shop preset by vnum
    pub fn get_shop_preset_by_vnum(&self, vnum: &str) -> Result<Option<ShopPreset>> {
        let vnum_lower = vnum.to_lowercase();
        for entry in self.shop_presets.iter() {
            let (_key, value) = entry?;
            let preset: ShopPreset = serde_json::from_slice(&value)?;
            if preset.vnum.to_lowercase() == vnum_lower {
                return Ok(Some(preset));
            }
        }
        Ok(None)
    }

    /// Save shop preset
    pub fn save_shop_preset(&self, preset: &ShopPreset) -> Result<()> {
        let key = preset.id.as_bytes();
        let value = serde_json::to_vec(preset)?;
        self.shop_presets.insert(key, value)?;
        Ok(())
    }

    /// Delete a shop preset
    pub fn delete_shop_preset(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.shop_presets.remove(key)?.is_some())
    }

    /// List all shop presets
    pub fn list_all_shop_presets(&self) -> Result<Vec<ShopPreset>> {
        let mut presets = Vec::new();
        for entry in self.shop_presets.iter() {
            let (_key, value) = entry?;
            let preset: ShopPreset = serde_json::from_slice(&value)?;
            presets.push(preset);
        }
        Ok(presets)
    }

    // ========== Lease Functions ==========

    /// Get lease by ID
    pub fn get_lease(&self, id: &Uuid) -> Result<Option<LeaseData>> {
        let key = id.as_bytes();
        match self.leases.get(key)? {
            Some(ivec) => {
                let lease: LeaseData = serde_json::from_slice(&ivec)?;
                Ok(Some(lease))
            }
            None => Ok(None),
        }
    }

    /// Save lease data
    pub fn save_lease(&self, lease: &LeaseData) -> Result<()> {
        let key = lease.id.as_bytes();
        let value = serde_json::to_vec(lease)?;
        self.leases.insert(key, value)?;
        Ok(())
    }

    /// Delete a lease
    pub fn delete_lease(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.leases.remove(key)?.is_some())
    }

    /// List all leases
    pub fn list_all_leases(&self) -> Result<Vec<LeaseData>> {
        let mut leases = Vec::new();
        for entry in self.leases.iter() {
            let (_key, value) = entry?;
            let lease: LeaseData = serde_json::from_slice(&value)?;
            leases.push(lease);
        }
        Ok(leases)
    }

    /// Get all leases for a player
    pub fn get_leases_by_owner(&self, owner_name: &str) -> Result<Vec<LeaseData>> {
        let name_lower = owner_name.to_lowercase();
        let mut leases = Vec::new();
        for entry in self.leases.iter() {
            let (_key, value) = entry?;
            let lease: LeaseData = serde_json::from_slice(&value)?;
            if lease.owner_name.to_lowercase() == name_lower && !lease.is_evicted {
                leases.push(lease);
            }
        }
        Ok(leases)
    }

    /// Get player's lease in a specific area
    pub fn get_player_lease_in_area(&self, owner_name: &str, area_id: &Uuid) -> Result<Option<LeaseData>> {
        let name_lower = owner_name.to_lowercase();
        for entry in self.leases.iter() {
            let (_key, value) = entry?;
            let lease: LeaseData = serde_json::from_slice(&value)?;
            if lease.owner_name.to_lowercase() == name_lower && lease.area_id == *area_id && !lease.is_evicted {
                return Ok(Some(lease));
            }
        }
        Ok(None)
    }

    /// Count active leases for a template
    pub fn count_template_instances(&self, template_vnum: &str) -> Result<i32> {
        let vnum_lower = template_vnum.to_lowercase();
        let mut count = 0;
        for entry in self.leases.iter() {
            let (_key, value) = entry?;
            let lease: LeaseData = serde_json::from_slice(&value)?;
            if lease.template_vnum.to_lowercase() == vnum_lower && !lease.is_evicted {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Get lease for a property room
    pub fn get_lease_for_room(&self, room_id: &Uuid) -> Result<Option<LeaseData>> {
        // First check if this room is a property room
        if let Some(room) = self.get_room_data(room_id)? {
            if let Some(lease_id) = room.property_lease_id {
                return self.get_lease(&lease_id);
            }
        }
        Ok(None)
    }

    // ========== Escrow Functions ==========

    /// Get escrow by ID
    pub fn get_escrow(&self, id: &Uuid) -> Result<Option<EscrowData>> {
        let key = id.as_bytes();
        match self.escrow.get(key)? {
            Some(ivec) => {
                let escrow: EscrowData = serde_json::from_slice(&ivec)?;
                Ok(Some(escrow))
            }
            None => Ok(None),
        }
    }

    /// Save escrow data
    pub fn save_escrow(&self, escrow: &EscrowData) -> Result<()> {
        let key = escrow.id.as_bytes();
        let value = serde_json::to_vec(escrow)?;
        self.escrow.insert(key, value)?;
        Ok(())
    }

    /// Delete an escrow
    pub fn delete_escrow(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.escrow.remove(key)?.is_some())
    }

    /// List all escrow entries
    pub fn list_all_escrow(&self) -> Result<Vec<EscrowData>> {
        let mut escrows = Vec::new();
        for entry in self.escrow.iter() {
            let (_key, value) = entry?;
            let escrow: EscrowData = serde_json::from_slice(&value)?;
            escrows.push(escrow);
        }
        Ok(escrows)
    }

    /// Get all escrow entries for a player
    pub fn get_escrow_by_owner(&self, owner_name: &str) -> Result<Vec<EscrowData>> {
        let name_lower = owner_name.to_lowercase();
        let mut escrows = Vec::new();
        for entry in self.escrow.iter() {
            let (_key, value) = entry?;
            let escrow: EscrowData = serde_json::from_slice(&value)?;
            if escrow.owner_name.to_lowercase() == name_lower {
                escrows.push(escrow);
            }
        }
        Ok(escrows)
    }

    // === API Key Methods ===

    /// Save an API key
    pub fn save_api_key(&self, key: &ApiKey) -> Result<()> {
        let db_key = key.id.as_bytes();
        let value = serde_json::to_vec(key)?;
        self.api_keys.insert(db_key, value)?;
        Ok(())
    }

    /// Get an API key by ID
    pub fn get_api_key(&self, id: &Uuid) -> Result<Option<ApiKey>> {
        let key = id.as_bytes();
        match self.api_keys.get(key)? {
            Some(ivec) => {
                let api_key: ApiKey = serde_json::from_slice(&ivec)?;
                Ok(Some(api_key))
            }
            None => Ok(None),
        }
    }

    /// Find an API key by checking against all stored hashes
    /// This is used during authentication when we receive the raw key
    pub fn find_api_key_by_raw_key(&self, raw_key: &str) -> Result<Option<ApiKey>> {
        for entry in self.api_keys.iter() {
            let (_db_key, value) = entry?;
            let api_key: ApiKey = serde_json::from_slice(&value)?;
            // Verify the raw key against the stored hash
            if self.verify_password(raw_key, &api_key.key_hash)? {
                return Ok(Some(api_key));
            }
        }
        Ok(None)
    }

    /// List all API keys
    pub fn list_all_api_keys(&self) -> Result<Vec<ApiKey>> {
        let mut keys = Vec::new();
        for entry in self.api_keys.iter() {
            let (_key, value) = entry?;
            let api_key: ApiKey = serde_json::from_slice(&value)?;
            keys.push(api_key);
        }
        Ok(keys)
    }

    /// Delete an API key by ID
    pub fn delete_api_key(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.api_keys.remove(key)?.is_some())
    }

    /// Update an API key's last_used timestamp
    pub fn update_api_key_last_used(&self, id: &Uuid) -> Result<()> {
        if let Some(mut api_key) = self.get_api_key(id)? {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            api_key.last_used_at = Some(now);
            self.save_api_key(&api_key)?;
        }
        Ok(())
    }

    // === Mail System Methods ===

    /// Store a new mail message
    pub fn store_mail(&self, message: MailMessage) -> Result<()> {
        let key = message.id.as_bytes();
        let value = serde_json::to_vec(&message)?;
        self.mail.insert(key, value)?;
        Ok(())
    }

    /// Get all mail for a recipient (sorted by sent_at, newest first)
    pub fn get_mail_for_recipient(&self, recipient: &str) -> Result<Vec<MailMessage>> {
        let recipient_lower = recipient.to_lowercase();
        let mut messages = Vec::new();
        for entry in self.mail.iter() {
            let (_key, value) = entry?;
            let msg: MailMessage = serde_json::from_slice(&value)?;
            if msg.recipient == recipient_lower {
                messages.push(msg);
            }
        }
        // Sort by sent_at descending (newest first)
        messages.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        Ok(messages)
    }

    /// Get count of unread mail for a recipient
    pub fn get_unread_mail_count(&self, recipient: &str) -> Result<i64> {
        let recipient_lower = recipient.to_lowercase();
        let mut count = 0i64;
        for entry in self.mail.iter() {
            let (_key, value) = entry?;
            let msg: MailMessage = serde_json::from_slice(&value)?;
            if msg.recipient == recipient_lower && !msg.read {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Get total mailbox size for a recipient
    pub fn get_mailbox_size(&self, recipient: &str) -> Result<i64> {
        let recipient_lower = recipient.to_lowercase();
        let mut count = 0i64;
        for entry in self.mail.iter() {
            let (_key, value) = entry?;
            let msg: MailMessage = serde_json::from_slice(&value)?;
            if msg.recipient == recipient_lower {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Mark a mail message as read
    pub fn mark_mail_read(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        if let Some(ivec) = self.mail.get(key)? {
            let mut msg: MailMessage = serde_json::from_slice(&ivec)?;
            msg.read = true;
            let value = serde_json::to_vec(&msg)?;
            self.mail.insert(key, value)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Delete a mail message by ID
    pub fn delete_mail(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.mail.remove(key)?.is_some())
    }

    /// Get a specific mail message by ID
    pub fn get_mail_by_id(&self, id: &Uuid) -> Result<Option<MailMessage>> {
        let key = id.as_bytes();
        match self.mail.get(key)? {
            Some(ivec) => {
                let msg: MailMessage = serde_json::from_slice(&ivec)?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Delete the oldest read message for a recipient (for auto-cleanup)
    /// Returns true if a message was deleted, false if no read messages exist
    pub fn delete_oldest_read_mail(&self, recipient: &str) -> Result<bool> {
        let recipient_lower = recipient.to_lowercase();
        let mut oldest_read: Option<(Uuid, i64)> = None;

        for entry in self.mail.iter() {
            let (_key, value) = entry?;
            let msg: MailMessage = serde_json::from_slice(&value)?;
            if msg.recipient == recipient_lower && msg.read {
                match oldest_read {
                    None => oldest_read = Some((msg.id, msg.sent_at)),
                    Some((_, oldest_time)) if msg.sent_at < oldest_time => {
                        oldest_read = Some((msg.id, msg.sent_at));
                    }
                    _ => {}
                }
            }
        }

        if let Some((id, _)) = oldest_read {
            return self.delete_mail(&id);
        }
        Ok(false)
    }

    /// Check if all messages in mailbox are unread
    pub fn all_mail_unread(&self, recipient: &str) -> Result<bool> {
        let recipient_lower = recipient.to_lowercase();
        for entry in self.mail.iter() {
            let (_key, value) = entry?;
            let msg: MailMessage = serde_json::from_slice(&value)?;
            if msg.recipient == recipient_lower && msg.read {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // ========== Bug Reporting System Functions ==========

    /// Get the next sequential bug ticket number (atomic increment)
    pub fn next_bug_ticket_number(&self) -> Result<i64> {
        // Try to get current counter from settings
        let current = self
            .get_setting("bug_ticket_counter")?
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        // Safety check: scan existing reports for max ticket number
        let mut max_existing = 0i64;
        for entry in self.bug_reports.iter() {
            let (_key, value) = entry?;
            let report: crate::BugReport = serde_json::from_slice(&value)?;
            if report.ticket_number > max_existing {
                max_existing = report.ticket_number;
            }
        }

        let next = std::cmp::max(current, max_existing) + 1;
        self.set_setting("bug_ticket_counter", &next.to_string())?;
        Ok(next)
    }

    /// Store a new bug report
    pub fn store_bug_report(&self, report: crate::BugReport) -> Result<()> {
        let key = report.id.as_bytes();
        let value = serde_json::to_vec(&report)?;
        self.bug_reports.insert(key, value)?;
        Ok(())
    }

    /// Get a bug report by UUID
    pub fn get_bug_report(&self, id: &Uuid) -> Result<Option<crate::BugReport>> {
        match self.bug_reports.get(id.as_bytes())? {
            Some(ivec) => {
                let report: crate::BugReport = serde_json::from_slice(&ivec)?;
                Ok(Some(report))
            }
            None => Ok(None),
        }
    }

    /// Get a bug report by ticket number (iteration scan)
    pub fn get_bug_report_by_ticket(&self, ticket_number: i64) -> Result<Option<crate::BugReport>> {
        for entry in self.bug_reports.iter() {
            let (_key, value) = entry?;
            let report: crate::BugReport = serde_json::from_slice(&value)?;
            if report.ticket_number == ticket_number {
                return Ok(Some(report));
            }
        }
        Ok(None)
    }

    /// List bug reports with optional status filter and approval filter
    /// When approved_only=true, only returns approved reports (for API/MCP)
    pub fn list_bug_reports(
        &self,
        status_filter: Option<&crate::BugStatus>,
        approved_only: bool,
    ) -> Result<Vec<crate::BugReport>> {
        let mut reports = Vec::new();
        for entry in self.bug_reports.iter() {
            let (_key, value) = entry?;
            let report: crate::BugReport = serde_json::from_slice(&value)?;
            if approved_only && !report.approved {
                continue;
            }
            if let Some(filter) = status_filter {
                if &report.status != filter {
                    continue;
                }
            }
            reports.push(report);
        }
        // Sort by created_at descending (newest first)
        reports.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(reports)
    }

    /// Save/update an existing bug report
    pub fn save_bug_report(&self, report: crate::BugReport) -> Result<()> {
        let key = report.id.as_bytes();
        let value = serde_json::to_vec(&report)?;
        self.bug_reports.insert(key, value)?;
        Ok(())
    }

    /// Delete a bug report by UUID
    pub fn delete_bug_report(&self, id: &Uuid) -> Result<bool> {
        Ok(self.bug_reports.remove(id.as_bytes())?.is_some())
    }

    /// Count open bug reports (Open + InProgress)
    pub fn count_open_bug_reports(&self) -> Result<i64> {
        let mut count = 0i64;
        for entry in self.bug_reports.iter() {
            let (_key, value) = entry?;
            let report: crate::BugReport = serde_json::from_slice(&value)?;
            if report.status == crate::BugStatus::Open || report.status == crate::BugStatus::InProgress {
                count += 1;
            }
        }
        Ok(count)
    }

    // ========== Gardening System Functions ==========

    /// Get a plant instance by UUID
    pub fn get_plant(&self, plant_id: &Uuid) -> Result<Option<PlantInstance>> {
        match self.plants.get(plant_id.as_bytes())? {
            Some(ivec) => {
                let plant: PlantInstance = serde_json::from_slice(&ivec)?;
                Ok(Some(plant))
            }
            None => Ok(None),
        }
    }

    /// Save a plant instance
    pub fn save_plant(&self, plant: PlantInstance) -> Result<()> {
        let value = serde_json::to_vec(&plant)?;
        self.plants.insert(plant.id.as_bytes(), value)?;
        Ok(())
    }

    /// Delete a plant instance
    pub fn delete_plant(&self, plant_id: &Uuid) -> Result<bool> {
        Ok(self.plants.remove(plant_id.as_bytes())?.is_some())
    }

    /// List all plant instances
    pub fn list_all_plants(&self) -> Result<Vec<PlantInstance>> {
        let mut plants = Vec::new();
        for entry in self.plants.iter() {
            let (_key, value) = entry?;
            let plant: PlantInstance = serde_json::from_slice(&value)?;
            plants.push(plant);
        }
        Ok(plants)
    }

    /// Get all plants in a specific room
    pub fn get_plants_in_room(&self, room_id: &Uuid) -> Result<Vec<PlantInstance>> {
        let mut plants = Vec::new();
        for entry in self.plants.iter() {
            let (_key, value) = entry?;
            let plant: PlantInstance = serde_json::from_slice(&value)?;
            if plant.room_id == *room_id {
                plants.push(plant);
            }
        }
        Ok(plants)
    }

    /// Get a plant prototype by UUID
    pub fn get_plant_prototype(&self, proto_id: &Uuid) -> Result<Option<PlantPrototype>> {
        match self.plant_prototypes.get(proto_id.as_bytes())? {
            Some(ivec) => {
                let proto: PlantPrototype = serde_json::from_slice(&ivec)?;
                Ok(Some(proto))
            }
            None => Ok(None),
        }
    }

    /// Get a plant prototype by vnum
    pub fn get_plant_prototype_by_vnum(&self, vnum: &str) -> Result<Option<PlantPrototype>> {
        for entry in self.plant_prototypes.iter() {
            let (_key, value) = entry?;
            let proto: PlantPrototype = serde_json::from_slice(&value)?;
            if proto.vnum.as_deref() == Some(vnum) {
                return Ok(Some(proto));
            }
        }
        Ok(None)
    }

    /// Save a plant prototype
    pub fn save_plant_prototype(&self, proto: PlantPrototype) -> Result<()> {
        let value = serde_json::to_vec(&proto)?;
        self.plant_prototypes.insert(proto.id.as_bytes(), value)?;
        Ok(())
    }

    /// Delete a plant prototype
    pub fn delete_plant_prototype(&self, id: &Uuid) -> Result<bool> {
        Ok(self.plant_prototypes.remove(id.as_bytes())?.is_some())
    }

    /// List all plant prototypes
    pub fn list_all_plant_prototypes(&self) -> Result<Vec<PlantPrototype>> {
        let mut protos = Vec::new();
        for entry in self.plant_prototypes.iter() {
            let (_key, value) = entry?;
            let proto: PlantPrototype = serde_json::from_slice(&value)?;
            protos.push(proto);
        }
        Ok(protos)
    }

    // ========== World Management ==========

    /// Get counts of all entity types in the database
    pub fn world_stats(&self) -> Result<WorldStats> {
        Ok(WorldStats {
            areas: self.areas.len(),
            rooms: self.rooms.len(),
            items: self.items.len(),
            mobiles: self.mobiles.len(),
            spawn_points: self.spawn_points.len(),
            recipes: self.recipes.len(),
            transports: self.transports.len(),
            property_templates: self.property_templates.len(),
            leases: self.leases.len(),
            plant_prototypes: self.plant_prototypes.len(),
            plants: self.plants.len(),
            characters: self.characters.len(),
        })
    }

    /// Clear all world data except characters, settings, and API keys.
    /// Resets all character `current_room_id` to STARTING_ROOM_ID.
    pub fn clear_world_data(&self) -> Result<()> {
        let starting_room = Uuid::parse_str(STARTING_ROOM_ID)?;

        // Clear world entity trees
        self.rooms.clear()?;
        self.vnum_index.clear()?;
        self.areas.clear()?;
        self.items.clear()?;
        self.mobiles.clear()?;
        self.spawn_points.clear()?;
        self.recipes.clear()?;
        self.transports.clear()?;
        self.property_templates.clear()?;
        self.leases.clear()?;
        self.escrow.clear()?;
        self.shop_presets.clear()?;
        self.mail.clear()?;
        self.plants.clear()?;
        self.plant_prototypes.clear()?;

        // Reset all characters to starting room and clear property data
        let mut chars_to_update = Vec::new();
        for entry in self.characters.iter() {
            let (_key, value) = entry?;
            let mut character: CharacterData = serde_json::from_slice(&value)?;
            character.current_room_id = starting_room;
            character.active_leases.clear();
            character.escrow_ids.clear();
            character.tour_origin_room = None;
            character.on_tour = false;
            chars_to_update.push(character);
        }
        for character in chars_to_update {
            self.save_character_data(character)?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal CAS helper for update_* methods
// ---------------------------------------------------------------------------

/// Compare-and-swap based read-modify-write on a sled `Tree`. Retries the
/// closure when another writer beats us to the punch. Returns the
/// post-mutation entity, or `None` if the key didn't exist at the start of
/// the final successful attempt.
///
/// Public `update_mobile` / `update_character` / `update_room` / `update_item`
/// methods on `Db` are thin wrappers around this.
fn update_tree<T, F>(tree: &Tree, key: &[u8], mut f: F) -> Result<Option<T>>
where
    T: for<'de> serde::Deserialize<'de> + serde::Serialize,
    F: FnMut(&mut T),
{
    loop {
        let current = tree.get(key)?;
        let old_bytes = match &current {
            Some(iv) => iv.clone(),
            None => return Ok(None),
        };
        let mut entity: T = serde_json::from_slice(&old_bytes)?;
        f(&mut entity);
        let new_bytes = serde_json::to_vec(&entity)?;

        match tree.compare_and_swap(key, Some(&old_bytes), Some(new_bytes.as_slice()))? {
            Ok(()) => return Ok(Some(entity)),
            Err(_conflict) => {
                // Another writer committed between our read and our CAS;
                // reload and retry. The closure will be re-invoked on a
                // fresh copy, which is why callers must keep side effects
                // outside.
                continue;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDb {
        db: Db,
        path: String,
    }
    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
    fn open_temp(tag: &str) -> TempDb {
        let path = format!(
            "test_update_{}_{}_{}.db",
            tag,
            std::process::id(),
            Uuid::new_v4().simple()
        );
        let _ = std::fs::remove_dir_all(&path);
        let db = Db::open(&path).expect("open db");
        TempDb { db, path }
    }

    #[test]
    fn update_mobile_applies_closure_and_returns_snapshot() {
        let t = open_temp("apply");
        let mut m = MobileData::new("Tester".to_string());
        m.gold = 10;
        let id = m.id;
        t.db.save_mobile_data(m).expect("save");

        let result = t.db.update_mobile(&id, |m| m.gold += 5).expect("update");
        let post = result.expect("mobile still exists");
        assert_eq!(post.gold, 15);

        let reloaded = t.db.get_mobile_data(&id).unwrap().unwrap();
        assert_eq!(reloaded.gold, 15);
    }

    #[test]
    fn update_mobile_returns_none_for_missing() {
        let t = open_temp("missing");
        let id = Uuid::new_v4();
        let result = t.db.update_mobile(&id, |m| m.gold += 5).expect("update");
        assert!(result.is_none());
    }

    #[test]
    fn update_mobile_retries_on_concurrent_writer() {
        // Simulate a concurrent write by having the closure itself perform
        // a direct save the first time it runs. The CAS on our outer loop
        // will fail, the closure will be re-invoked on a fresh snapshot,
        // and the final state should reflect BOTH writes — the concurrent
        // one and the closure's own mutation.
        let t = open_temp("retry");
        let mut m = MobileData::new("Racer".to_string());
        m.gold = 100;
        let id = m.id;
        t.db.save_mobile_data(m).expect("save");

        let db2 = t.db.clone();
        let mut first_call = true;
        let result =
            t.db.update_mobile(&id, |m| {
                if first_call {
                    // Inject a concurrent modification: bump gold by 1 via
                    // a direct save. Our pending CAS should fail, we retry,
                    // and next iteration starts from this new state.
                    first_call = false;
                    let mut sneaky = db2.get_mobile_data(&id).unwrap().unwrap();
                    sneaky.gold += 1;
                    db2.save_mobile_data(sneaky).unwrap();
                }
                // In any attempt, bump gold by 10.
                m.gold += 10;
            })
            .expect("update");

        let post = result.expect("mobile still exists");
        // Concurrent writer added 1, our closure's surviving attempt added 10.
        assert_eq!(post.gold, 111);
        let reloaded = t.db.get_mobile_data(&id).unwrap().unwrap();
        assert_eq!(reloaded.gold, 111);
    }
}
