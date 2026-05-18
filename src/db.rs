use anyhow::Result;
use serde_json;
use sled::{Db as SledDb, Tree};
use std::path::Path;
use std::sync::Arc; // Import Arc

use crate::{
    AchievementDef, ApiKey, AreaData, BoardPost, CharacterData, EscrowData, ItemData, ItemLocation, ItemType,
    LeaseData, MailMessage, MobileData, PlantInstance, PlantPrototype, PropertyTemplate, Recipe, RoomData,
    STARTING_ROOM_ID, ShopPreset, SpawnEntityType, SpawnPointData, TransportData,
};
use uuid::Uuid;

/// Hard cap on password length accepted by `hash_password` / `verify_password`.
/// Argon2 cost is roughly linear in input size; without a cap, a single
/// pre-auth login attempt with a multi-MB password stalls a CPU core.
pub const MAX_PASSWORD_LEN: usize = 128;

#[derive(Clone)] // Derive Clone
pub struct Db {
    db: Arc<SledDb>,       // Use Arc
    characters: Arc<Tree>, // Use Arc
    // Account auth + character roster. Key = lowercase name. Migrated from
    // characters at first run (one account per pre-feature character).
    accounts: Arc<Tree>,
    // Account UUID → lowercase name pointer (mirrors character_id_index pattern
    // used elsewhere — keeps id-based lookups O(1)).
    account_id_index: Arc<Tree>,
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
    // Bulletin boards (gen_board parity). One flat tree keyed by
    // `BoardPost.id`; per-board lookup scans + filters by `board_vnum`.
    boards: Arc<Tree>,
    // Gardening system
    plants: Arc<Tree>,
    plant_prototypes: Arc<Tree>,
    // Bug reporting system
    bug_reports: Arc<Tree>,
    // DG Scripts global variable store (key = var name, value = string).
    // Backs `global <var>` declarations in the DG interpreter.
    dg_globals: Arc<Tree>,
    // DG Scripts trigger prototypes (key = vnum, value = serialized
    // DgTriggerProto). Imported `.trg` files seed this; the `attach`
    // statement and `trigger dg attach` builder command read from it.
    dg_trigger_protos: Arc<Tree>,
    // Achievement definitions authored via achedit / REST / MCP.
    // Canonical engine-detected achievements live in JSON; this tree
    // stores builder-created Manual ones and admin overrides.
    achievements: Arc<Tree>,
    // Quest prototypes (key = vnum bytes, value = JSON QuestData). Per-player
    // progress lives on CharacterData; this tree only holds prototypes.
    quests: Arc<Tree>,
    // Site (IP-level) bans. Key = canonical IP string ("a.b.c.d" or v6 form),
    // value = JSON `SiteBanRecord`. Read pre-auth in the TCP accept loop.
    bans: Arc<Tree>,
    // Per-IP account history for evasion detection. Key =
    // `<ip>\0<account_id>`, value = unix-secs `i64` of last seen. Range-scan
    // by `<ip>\0` prefix to find accounts sharing an IP. GC'd lazily on read
    // (drop entries older than 30 days).
    ip_account_history: Arc<Tree>,
    // Bounded ring of outbound-email audit entries. Key = u64 big-endian
    // monotonic id, value = JSON `EmailAuditEntry`. Trimmed to the most
    // recent EMAIL_AUDIT_RING_SIZE rows on every insert.
    email_audit: Arc<Tree>,
    // Per-class starting kit overrides (gold + item vnums) authored via
    // `cedit`. Overlays JSON-loaded ClassDefinition fields at startup; JSON
    // remains the source of truth for skills/bonuses/languages.
    class_loadouts: Arc<Tree>,
}

/// Maximum entries retained in the email-audit ring. One row per send
/// attempt — outcome included, so quota refusals and SMTP failures are
/// preserved alongside successes for incident review.
pub const EMAIL_AUDIT_RING_SIZE: usize = 1000;

/// One row in the email-audit ring.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmailAuditEntry {
    pub timestamp: i64,
    /// "verification" or "reset" today; extend if more flows ship.
    pub kind: String,
    /// Account name when known; "" otherwise. We deliberately do NOT log
    /// audit entries for misses on the forgot flow, so this is non-empty in
    /// practice — but kept optional in the schema for future flows.
    pub account_name: String,
    /// "sent" / "quota_daily" / "quota_monthly" / "smtp_failed" / "config_missing".
    pub outcome: String,
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
        let accounts = db.open_tree("accounts")?;
        let account_id_index = db.open_tree("account_id_index")?;
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
        let boards = db.open_tree("boards")?;
        let plants = db.open_tree("plants")?;
        let plant_prototypes = db.open_tree("plant_prototypes")?;
        let bug_reports = db.open_tree("bug_reports")?;
        let dg_globals = db.open_tree("dg_globals")?;
        let dg_trigger_protos = db.open_tree("dg_trigger_protos")?;
        let achievements = db.open_tree("achievements")?;
        let quests = db.open_tree("quests")?;
        let bans = db.open_tree("bans")?;
        let ip_account_history = db.open_tree("ip_account_history")?;
        let email_audit = db.open_tree("email_audit")?;
        let class_loadouts = db.open_tree("class_loadouts")?;
        let me = Self {
            db: Arc::new(db),                 // Wrap in Arc
            characters: Arc::new(characters), // Wrap in Arc
            accounts: Arc::new(accounts),
            account_id_index: Arc::new(account_id_index),
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
            boards: Arc::new(boards),
            plants: Arc::new(plants),
            plant_prototypes: Arc::new(plant_prototypes),
            bug_reports: Arc::new(bug_reports),
            dg_globals: Arc::new(dg_globals),
            dg_trigger_protos: Arc::new(dg_trigger_protos),
            achievements: Arc::new(achievements),
            quests: Arc::new(quests),
            bans: Arc::new(bans),
            ip_account_history: Arc::new(ip_account_history),
            email_audit: Arc::new(email_audit),
            class_loadouts: Arc::new(class_loadouts),
        };
        // One-shot migration: synthesize a 1:1 Account for every pre-feature
        // character that doesn't already have one. Idempotent.
        me.run_account_migration_if_needed()?;
        // One-shot backfill: compute `normalized_email` for accounts that
        // have an `email` set but were saved before the ban-tooling slice.
        me.run_normalized_email_backfill_if_needed()?;
        Ok(me)
    }

    // === DG Scripts global variables ===
    //
    // Backed by the `dg_globals` sled tree. Values persist across reboots,
    // matching tbamud's expectation that `global <var>` is durable. All
    // values are stored as plain strings (DG is dynamically typed).

    /// Read a DG global variable. Returns `None` when unset.
    pub fn get_dg_global(&self, name: &str) -> Result<Option<String>> {
        let res = self.dg_globals.get(name.as_bytes())?;
        Ok(res.and_then(|v| String::from_utf8(v.to_vec()).ok()))
    }

    /// Set (or overwrite) a DG global variable.
    pub fn set_dg_global(&self, name: &str, value: &str) -> Result<()> {
        self.dg_globals.insert(name.as_bytes(), value.as_bytes())?;
        Ok(())
    }

    /// Remove a DG global variable. No-op if not present.
    pub fn unset_dg_global(&self, name: &str) -> Result<()> {
        self.dg_globals.remove(name.as_bytes())?;
        Ok(())
    }

    // === DG Scripts trigger prototypes ===

    /// Read a DG trigger prototype by vnum.
    pub fn get_dg_trigger_proto(&self, vnum: &str) -> Result<Option<crate::types::DgTriggerProto>> {
        match self.dg_trigger_protos.get(vnum.as_bytes())? {
            Some(ivec) => Ok(Some(serde_json::from_slice(&ivec)?)),
            None => Ok(None),
        }
    }

    /// Save (or overwrite) a DG trigger prototype.
    pub fn save_dg_trigger_proto(&self, proto: &crate::types::DgTriggerProto) -> Result<()> {
        let value = serde_json::to_vec(proto)?;
        self.dg_trigger_protos.insert(proto.vnum.as_bytes(), value)?;
        Ok(())
    }

    /// List all DG trigger prototypes (e.g. for `trigger dg list`).
    pub fn list_dg_trigger_protos(&self) -> Result<Vec<crate::types::DgTriggerProto>> {
        let mut out = Vec::new();
        for kv in self.dg_trigger_protos.iter() {
            let (_, ivec) = kv?;
            if let Ok(p) = serde_json::from_slice::<crate::types::DgTriggerProto>(&ivec) {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// Delete a DG trigger prototype by vnum.
    pub fn delete_dg_trigger_proto(&self, vnum: &str) -> Result<()> {
        self.dg_trigger_protos.remove(vnum.as_bytes())?;
        Ok(())
    }

    /// Orphan every attached instance of `vnum`: clear `source_proto_vnum`
    /// on triggers across mobs/items/rooms whose attach_kind matches the
    /// proto. Bodies are preserved — instances become host-local triggers
    /// that no longer refresh on proto edits. Returns the number of trigger
    /// instances orphaned (not entities).
    ///
    /// Called by `trigger dg proto delete` to implement the "Orphan" delete
    /// policy: safer than auto-deleting instances since their behavior is
    /// preserved until a builder explicitly edits or removes them.
    pub fn orphan_attached_dg_triggers(&self, proto_vnum: &str) -> Result<usize> {
        use crate::types::DgAttachKind;
        let proto = match self.get_dg_trigger_proto(proto_vnum)? {
            Some(p) => p,
            None => return Ok(0),
        };
        let mut orphaned = 0usize;
        let target = Some(proto_vnum);
        match proto.attach_kind {
            DgAttachKind::Mob => {
                for mob in self.list_all_mobiles()? {
                    if !mob.triggers.iter().any(|t| t.source_proto_vnum.as_deref() == target) {
                        continue;
                    }
                    self.update_mobile(&mob.id, |m| {
                        for t in &mut m.triggers {
                            if t.source_proto_vnum.as_deref() == target {
                                t.source_proto_vnum = None;
                                orphaned += 1;
                            }
                        }
                    })?;
                }
            }
            DgAttachKind::Obj => {
                for item in self.list_all_items()? {
                    if !item.triggers.iter().any(|t| t.source_proto_vnum.as_deref() == target) {
                        continue;
                    }
                    self.update_item(&item.id, |i| {
                        for t in &mut i.triggers {
                            if t.source_proto_vnum.as_deref() == target {
                                t.source_proto_vnum = None;
                                orphaned += 1;
                            }
                        }
                    })?;
                }
            }
            DgAttachKind::Room => {
                for room in self.list_all_rooms()? {
                    if !room.triggers.iter().any(|t| t.source_proto_vnum.as_deref() == target) {
                        continue;
                    }
                    self.update_room(&room.id, |r| {
                        for t in &mut r.triggers {
                            if t.source_proto_vnum.as_deref() == target {
                                t.source_proto_vnum = None;
                                orphaned += 1;
                            }
                        }
                    })?;
                }
            }
        }
        Ok(orphaned)
    }

    /// Sweep across all entities of matching `attach_kind` and rebuild any
    /// triggers whose `source_proto_vnum` matches the proto. Used by the
    /// proto save path — re-derives trigger types from current proto flags,
    /// drops every source-tagged trigger for this vnum, and pushes fresh
    /// per-type clones with the latest body/name/chance/args.
    ///
    /// Body, flag, name, chance, or arglist changes all propagate uniformly
    /// because the rebuild is total. Returns the count of entities updated
    /// (not triggers).
    pub fn refresh_attached_dg_triggers(
        &self,
        proto: &crate::types::DgTriggerProto,
    ) -> Result<usize> {
        use crate::import::engines::tba::trg_map;
        use crate::types::{DgAttachKind, ItemTrigger, MobileTrigger, RoomTrigger};
        let target = Some(proto.vnum.as_str());
        let mut entities_updated = 0usize;
        let chance = proto.numeric_arg.clamp(1, 100);
        let args: Vec<String> = if proto.arglist.trim().is_empty() {
            Vec::new()
        } else {
            proto
                .arglist
                .split_whitespace()
                .map(|w| w.to_string())
                .collect()
        };
        match proto.attach_kind {
            DgAttachKind::Mob => {
                let new_types = trg_map::mobile_trigger_types(&proto.flags);
                for mob in self.list_all_mobiles()? {
                    if !mob.triggers.iter().any(|t| t.source_proto_vnum.as_deref() == target) {
                        continue;
                    }
                    self.update_mobile(&mob.id, |m| {
                        m.triggers
                            .retain(|t| t.source_proto_vnum.as_deref() != target);
                        for ttype in &new_types {
                            m.triggers.push(MobileTrigger {
                                trigger_type: *ttype,
                                script_name: String::new(),
                                enabled: true,
                                chance,
                                args: args.clone(),
                                interval_secs: 60,
                                last_fired: 0,
                                dg_body: Some(proto.body.clone()),
                                dg_name: Some(proto.name.clone()),
                                authored_by: None,
                                elevated: false,
                                source_proto_vnum: Some(proto.vnum.clone()),
                            });
                        }
                    })?;
                    entities_updated += 1;
                }
            }
            DgAttachKind::Obj => {
                let new_types = trg_map::item_trigger_types(&proto.flags);
                for item in self.list_all_items()? {
                    if !item.triggers.iter().any(|t| t.source_proto_vnum.as_deref() == target) {
                        continue;
                    }
                    self.update_item(&item.id, |i| {
                        i.triggers
                            .retain(|t| t.source_proto_vnum.as_deref() != target);
                        for ttype in &new_types {
                            i.triggers.push(ItemTrigger {
                                trigger_type: *ttype,
                                script_name: String::new(),
                                enabled: true,
                                chance,
                                args: args.clone(),
                                dg_body: Some(proto.body.clone()),
                                dg_name: Some(proto.name.clone()),
                                authored_by: None,
                                elevated: false,
                                source_proto_vnum: Some(proto.vnum.clone()),
                            });
                        }
                    })?;
                    entities_updated += 1;
                }
            }
            DgAttachKind::Room => {
                let new_types = trg_map::room_trigger_types(&proto.flags);
                for room in self.list_all_rooms()? {
                    if !room.triggers.iter().any(|t| t.source_proto_vnum.as_deref() == target) {
                        continue;
                    }
                    self.update_room(&room.id, |r| {
                        r.triggers
                            .retain(|t| t.source_proto_vnum.as_deref() != target);
                        for ttype in &new_types {
                            r.triggers.push(RoomTrigger {
                                trigger_type: *ttype,
                                script_name: String::new(),
                                enabled: true,
                                interval_secs: 60,
                                last_fired: 0,
                                chance,
                                args: args.clone(),
                                dg_body: Some(proto.body.clone()),
                                dg_name: Some(proto.name.clone()),
                                authored_by: None,
                                elevated: false,
                                source_proto_vnum: Some(proto.vnum.clone()),
                            });
                        }
                    })?;
                    entities_updated += 1;
                }
            }
        }
        Ok(entities_updated)
    }

    /// Persist a proto and refresh all attached instances. Runs the DG
    /// analyzer first — any `ParseError` issue aborts the save (the proto
    /// is not written and attached instances are unchanged). Non-fatal
    /// issues (unknown commands/vars, etc.) are returned as warnings
    /// alongside the success report.
    ///
    /// Returns `Ok((entities_refreshed, warnings))` on success, or `Err`
    /// with a formatted parse-error message when the body is malformed.
    pub fn save_dg_trigger_proto_with_refresh(
        &self,
        proto: &crate::types::DgTriggerProto,
    ) -> Result<(usize, Vec<String>)> {
        use crate::script::dg::analyze::{analyze, IssueKind};
        let issues = analyze(&proto.body);
        let parse_errors: Vec<&_> = issues.iter().filter(|i| i.kind == IssueKind::ParseError).collect();
        if !parse_errors.is_empty() {
            let detail = parse_errors
                .iter()
                .map(|i| i.detail.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(anyhow::anyhow!("parse error: {}", detail));
        }
        let warnings: Vec<String> = issues
            .into_iter()
            .filter(|i| i.kind != IssueKind::ParseError)
            .map(|i| format!("{:?}: {}", i.kind, i.detail))
            .collect();
        self.save_dg_trigger_proto(proto)?;
        let refreshed = self.refresh_attached_dg_triggers(proto)?;
        Ok((refreshed, warnings))
    }

    // === Quest prototypes ===
    //
    // Backed by the `quests` sled tree. Stores `QuestData` keyed by vnum. Per-
    // player progress lives on `CharacterData.active_quests` /
    // `completed_quests` and rides the existing character save path.

    /// Read a quest prototype by vnum.
    pub fn get_quest_data(&self, vnum: &str) -> Result<Option<crate::types::QuestData>> {
        match self.quests.get(vnum.as_bytes())? {
            Some(ivec) => Ok(Some(serde_json::from_slice(&ivec)?)),
            None => Ok(None),
        }
    }

    /// Save (or overwrite) a quest prototype.
    pub fn save_quest_data(&self, quest: &crate::types::QuestData) -> Result<()> {
        let value = serde_json::to_vec(quest)?;
        self.quests.insert(quest.vnum.as_bytes(), value)?;
        Ok(())
    }

    /// Delete a quest prototype by vnum. No-op if not present.
    pub fn delete_quest(&self, vnum: &str) -> Result<()> {
        self.quests.remove(vnum.as_bytes())?;
        Ok(())
    }

    /// List all quest prototypes.
    pub fn list_all_quests(&self) -> Result<Vec<crate::types::QuestData>> {
        let mut out = Vec::new();
        for kv in self.quests.iter() {
            let (_, ivec) = kv?;
            if let Ok(q) = serde_json::from_slice::<crate::types::QuestData>(&ivec) {
                out.push(q);
            }
        }
        Ok(out)
    }

    /// Find quests whose canonical questgiver is the given mob vnum.
    pub fn find_quests_by_giver_mob_vnum(&self, mob_vnum: &str) -> Result<Vec<crate::types::QuestData>> {
        Ok(self
            .list_all_quests()?
            .into_iter()
            .filter(|q| q.giver_mob_vnum.as_deref() == Some(mob_vnum))
            .collect())
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
        // Purge any pending mail addressed to this recipient before removing
        // the character key — otherwise the messages orphan in the mail tree.
        let _ = self.delete_mail_for_recipient(name)?;
        let _ = self.delete_board_posts_by_author(name)?;
        // Remove this character's name from any owning account's roster so the
        // account's roster doesn't dangle past the character's lifetime.
        let _ = self.remove_character_name_from_any_account(name);
        self.characters.remove(key.as_bytes())?;
        Ok(())
    }

    // === Accounts ===
    //
    // The auth-bearing aggregate; each account owns 0+ characters. Source of
    // truth for `password_hash` post-migration. Keyed by lowercase name in the
    // `accounts` tree, with a parallel UUID→lowercase-name pointer in
    // `account_id_index` for fast id lookups.

    pub fn get_account(&self, name: &str) -> Result<Option<crate::types::AccountData>> {
        let key = name.to_lowercase();
        match self.accounts.get(key.as_bytes())? {
            Some(ivec) => Ok(Some(serde_json::from_slice(&ivec)?)),
            None => Ok(None),
        }
    }

    pub fn get_account_by_id(&self, id: &Uuid) -> Result<Option<crate::types::AccountData>> {
        match self.account_id_index.get(id.to_string().as_bytes())? {
            Some(ivec) => {
                let lower_name = String::from_utf8(ivec.to_vec()).map_err(|e| {
                    anyhow::anyhow!("account_id_index value not UTF-8: {}", e)
                })?;
                self.get_account(&lower_name)
            }
            None => Ok(None),
        }
    }

    pub fn save_account(&self, account: crate::types::AccountData) -> Result<()> {
        let key = account.name.to_lowercase();
        let value = serde_json::to_vec(&account)?;
        self.accounts.insert(key.as_bytes(), value)?;
        self.account_id_index
            .insert(account.id.to_string().as_bytes(), key.as_bytes())?;
        Ok(())
    }

    pub fn delete_account(&self, name: &str) -> Result<()> {
        let key = name.to_lowercase();
        if let Some(account) = self.get_account(&key)? {
            self.account_id_index
                .remove(account.id.to_string().as_bytes())?;
        }
        self.accounts.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<crate::types::AccountData>> {
        let mut out = Vec::new();
        for kv in self.accounts.iter() {
            let (_, ivec) = kv?;
            if let Ok(account) = serde_json::from_slice::<crate::types::AccountData>(&ivec) {
                out.push(account);
            }
        }
        Ok(out)
    }

    pub fn count_accounts(&self) -> Result<usize> {
        Ok(self.accounts.len())
    }

    /// Apply `delta` to `account.shared_bank_gold` and save. Refuses to drop
    /// the balance below zero (returns `Ok(None)` in that case). Returns the
    /// new balance on success.
    pub fn add_shared_bank_gold(
        &self,
        account_id: &Uuid,
        delta: i64,
    ) -> Result<Option<i64>> {
        let mut account = match self.get_account_by_id(account_id)? {
            Some(a) => a,
            None => return Ok(None),
        };
        let new_balance = account.shared_bank_gold.saturating_add(delta);
        if new_balance < 0 {
            return Ok(None);
        }
        account.shared_bank_gold = new_balance;
        self.save_account(account)?;
        Ok(Some(new_balance))
    }

    /// Replace `account.character_defaults` wholesale and save. The caller is
    /// expected to set `is_set = true` (or `false` for clear) on `prefs`
    /// before calling.
    pub fn save_account_preferences(
        &self,
        account_id: &Uuid,
        prefs: crate::types::AccountPreferences,
    ) -> Result<bool> {
        let mut account = match self.get_account_by_id(account_id)? {
            Some(a) => a,
            None => return Ok(false),
        };
        account.character_defaults = prefs;
        self.save_account(account)?;
        Ok(true)
    }

    pub fn add_character_to_account(
        &self,
        account_id: &Uuid,
        character_name: &str,
    ) -> Result<bool> {
        let mut account = match self.get_account_by_id(account_id)? {
            Some(a) => a,
            None => return Ok(false),
        };
        let already = account
            .character_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(character_name));
        if !already {
            account.character_names.push(character_name.to_string());
            self.save_account(account)?;
        }
        Ok(true)
    }

    pub fn remove_character_from_account(
        &self,
        account_id: &Uuid,
        character_name: &str,
    ) -> Result<bool> {
        let mut account = match self.get_account_by_id(account_id)? {
            Some(a) => a,
            None => return Ok(false),
        };
        let original_len = account.character_names.len();
        account
            .character_names
            .retain(|n| !n.eq_ignore_ascii_case(character_name));
        if account.character_names.len() != original_len {
            self.save_account(account)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Remove a character name from whatever account currently owns it. Used by
    /// `delete_character_data` so deleting a character also cleans up the
    /// account's roster pointer.
    pub fn remove_character_name_from_any_account(&self, character_name: &str) -> Result<bool> {
        for account in self.list_accounts()? {
            if account
                .character_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(character_name))
            {
                let id = account.id;
                return self.remove_character_from_account(&id, character_name);
            }
        }
        Ok(false)
    }

    /// Find an account by its email address (case-insensitive). Used by the
    /// email-verification slice to refuse duplicate registrations under one
    /// inbox. Linear scan — accounts are bounded by player count.
    pub fn find_account_by_email(
        &self,
        email: &str,
    ) -> Result<Option<crate::types::AccountData>> {
        let needle = email.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(None);
        }
        for account in self.list_accounts()? {
            if let Some(existing) = &account.email {
                if existing.trim().to_lowercase() == needle {
                    return Ok(Some(account));
                }
            }
        }
        Ok(None)
    }

    /// Find an account by its **normalized** email (Gmail dot/+tag stripping
    /// applied). The ban-tooling slice's evasion-detection layer compares on
    /// this canonical form so `agent.craig+spam@gmail.com` and
    /// `agentcraig@gmail.com` collide. Linear scan — same cost shape as
    /// `find_account_by_email`.
    pub fn find_account_by_normalized_email(
        &self,
        normalized: &str,
    ) -> Result<Option<crate::types::AccountData>> {
        let needle = normalized.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(None);
        }
        for account in self.list_accounts()? {
            if let Some(existing) = &account.normalized_email {
                if existing == &needle {
                    return Ok(Some(account));
                }
            }
        }
        Ok(None)
    }

    /// Insert / replace a site ban. Lazy-expiry: callers should treat the
    /// row as authoritative until `expires_at` passes — `get_site_ban`
    /// eagerly drops expired rows on read.
    pub fn put_site_ban(&self, record: &crate::types::SiteBanRecord) -> Result<()> {
        let key = record.ip.trim().to_lowercase();
        let bytes = serde_json::to_vec(record)?;
        self.bans.insert(key.as_bytes(), bytes)?;
        Ok(())
    }

    /// Remove a site ban. Returns true if a row existed.
    pub fn remove_site_ban(&self, ip: &str) -> Result<bool> {
        let key = ip.trim().to_lowercase();
        Ok(self.bans.remove(key.as_bytes())?.is_some())
    }

    /// Look up a site ban for an IP. Returns `None` for IPs that have never
    /// been banned, or whose ban has expired (the row is eagerly removed in
    /// the expired case so subsequent reads short-circuit cheaply).
    pub fn get_site_ban(&self, ip: &str) -> Result<Option<crate::types::SiteBanRecord>> {
        let key = ip.trim().to_lowercase();
        let Some(ivec) = self.bans.get(key.as_bytes())? else {
            return Ok(None);
        };
        let record: crate::types::SiteBanRecord = match serde_json::from_slice(&ivec) {
            Ok(r) => r,
            Err(_) => {
                let _ = self.bans.remove(key.as_bytes());
                return Ok(None);
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Some(expires) = record.expires_at {
            if now >= expires {
                let _ = self.bans.remove(key.as_bytes());
                return Ok(None);
            }
        }
        Ok(Some(record))
    }

    /// Enumerate every active site ban. Lazy-cleans expired rows during the
    /// scan so `admin sitebans` always shows fresh data without a separate
    /// reaper task.
    pub fn list_site_bans(&self) -> Result<Vec<crate::types::SiteBanRecord>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut out = Vec::new();
        let mut to_remove: Vec<Vec<u8>> = Vec::new();
        for kv in self.bans.iter() {
            let (k, v) = kv?;
            match serde_json::from_slice::<crate::types::SiteBanRecord>(&v) {
                Ok(r) => {
                    if let Some(expires) = r.expires_at {
                        if now >= expires {
                            to_remove.push(k.to_vec());
                            continue;
                        }
                    }
                    out.push(r);
                }
                Err(_) => to_remove.push(k.to_vec()),
            }
        }
        for k in to_remove {
            let _ = self.bans.remove(&k);
        }
        Ok(out)
    }

    /// Stamp `<ip>\0<account_id>` → now() in the ip_account_history tree.
    /// Idempotent: re-stamping just refreshes the timestamp.
    pub fn record_account_ip_seen(&self, account_id: Uuid, ip: &str) -> Result<()> {
        let ip = ip.trim().to_lowercase();
        if ip.is_empty() {
            return Ok(());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut key = Vec::with_capacity(ip.len() + 1 + 16);
        key.extend_from_slice(ip.as_bytes());
        key.push(0);
        key.extend_from_slice(account_id.as_bytes());
        self.ip_account_history.insert(key, &now.to_be_bytes())?;
        Ok(())
    }

    /// Return every account_id stamped against this IP whose `last_seen >=
    /// since_secs`. Lazy-cleans entries older than 30 days during the scan.
    pub fn list_accounts_by_ip(&self, ip: &str, since_secs: i64) -> Result<Vec<Uuid>> {
        let ip = ip.trim().to_lowercase();
        if ip.is_empty() {
            return Ok(Vec::new());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let stale_before = now - (30 * 24 * 3600);
        let mut prefix = Vec::with_capacity(ip.len() + 1);
        prefix.extend_from_slice(ip.as_bytes());
        prefix.push(0);
        let mut out = Vec::new();
        let mut to_remove: Vec<Vec<u8>> = Vec::new();
        for kv in self.ip_account_history.scan_prefix(&prefix) {
            let (k, v) = kv?;
            if v.len() != 8 {
                to_remove.push(k.to_vec());
                continue;
            }
            let mut ts_bytes = [0u8; 8];
            ts_bytes.copy_from_slice(&v);
            let ts = i64::from_be_bytes(ts_bytes);
            if ts < stale_before {
                to_remove.push(k.to_vec());
                continue;
            }
            if ts < since_secs {
                continue;
            }
            // key = <ip>\0<16 byte uuid>
            if k.len() < prefix.len() + 16 {
                to_remove.push(k.to_vec());
                continue;
            }
            let id_bytes = &k[prefix.len()..prefix.len() + 16];
            if let Ok(arr) = <[u8; 16]>::try_from(id_bytes) {
                out.push(Uuid::from_bytes(arr));
            }
        }
        for k in to_remove {
            let _ = self.ip_account_history.remove(&k);
        }
        Ok(out)
    }

    /// One-shot backfill: compute and persist `normalized_email` for every
    /// account that has `email = Some(_)` but `normalized_email = None`.
    /// Gated on the `accounts_normalized_email_backfilled` settings flag.
    fn run_normalized_email_backfill_if_needed(&self) -> Result<()> {
        if self
            .get_setting("accounts_normalized_email_backfilled")?
            .map(|v| v == "true")
            .unwrap_or(false)
        {
            return Ok(());
        }
        let mut backfilled = 0usize;
        for mut account in self.list_accounts()? {
            if account.normalized_email.is_some() {
                continue;
            }
            let Some(raw) = account.email.clone() else {
                continue;
            };
            let normalized = crate::email::normalize_email(&raw);
            if normalized.is_some() {
                account.normalized_email = normalized;
                self.save_account(account)?;
                backfilled += 1;
            }
        }
        self.set_setting("accounts_normalized_email_backfilled", "true")?;
        if backfilled > 0 {
            tracing::info!(
                "Account normalization backfill: stamped normalized_email on {backfilled} row(s)"
            );
        }
        Ok(())
    }

    /// Resolve which account owns this character name (linear scan; account
    /// counts are tiny compared to characters, so this is fine for now).
    pub fn find_account_for_character(
        &self,
        character_name: &str,
    ) -> Result<Option<crate::types::AccountData>> {
        for account in self.list_accounts()? {
            if account
                .character_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(character_name))
            {
                return Ok(Some(account));
            }
        }
        Ok(None)
    }

    /// One-shot migration: every pre-feature character with a non-empty
    /// password_hash gets a 1:1 Account row (`account.name = character.name`).
    /// Gated on the `accounts_migrated` setting flag — runs once per DB.
    fn run_account_migration_if_needed(&self) -> Result<()> {
        if self
            .get_setting("accounts_migrated")?
            .map(|v| v == "true")
            .unwrap_or(false)
        {
            return Ok(());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mut migrated = 0usize;
        for character in self.list_all_characters()? {
            if character.password_hash.is_empty() {
                continue;
            }
            let lower_name = character.name.to_lowercase();
            if self.get_account(&lower_name)?.is_some() {
                continue;
            }
            let account = crate::types::AccountData {
                id: Uuid::new_v4(),
                name: character.name.clone(),
                password_hash: character.password_hash.clone(),
                character_names: vec![character.name.clone()],
                email: None,
                email_verified: true,
                email_verification_code: None,
                email_verification_code_expires_at: 0,
                email_verification_last_sent_at: 0,
                email_verification_resend_count: 0,
                email_verification_resend_window_started_at: 0,
                email_verification_resend_day_count: 0,
                email_verification_resend_day_started_at: 0,
                password_reset_last_sent_at: 0,
                password_reset_window_started_at: 0,
                password_reset_count: 0,
                password_reset_day_count: 0,
                password_reset_day_started_at: 0,
                is_banned: false,
                ban_record: None,
                last_login_ip: String::new(),
                creation_ip: String::new(),
                normalized_email: None,
                created_at: now,
                last_login_at: 0,
                shared_bank_gold: 0,
                character_defaults: crate::types::AccountPreferences::default(),
            };
            self.save_account(account)?;
            migrated += 1;
        }
        self.set_setting("accounts_migrated", "true")?;
        if migrated > 0 {
            tracing::info!("Account migration: created {migrated} account row(s)");
        }
        Ok(())
    }

    // Hashing function
    pub fn hash_password(&self, password: &str) -> Result<String> {
        use argon2::{
            Argon2,
            password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
        };

        // Reject oversized passwords before they reach Argon2: a multi-MB
        // password would burn seconds of CPU per request, enabling pre-auth DoS.
        if password.len() > MAX_PASSWORD_LEN {
            anyhow::bail!("password exceeds maximum length");
        }

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

        // Same DoS gate as hash_password — if the input is bigger than any
        // legitimate password, fail-fast without invoking Argon2.
        if password.len() > MAX_PASSWORD_LEN {
            return Ok(false);
        }

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

    /// Resolve the climate preset for a room by walking to its area.
    /// Returns Temperate if the room has no area or the area lookup fails —
    /// keeping the no-area / orphaned-room case identical to the global
    /// pre-climate behavior.
    pub fn room_climate(&self, room: &RoomData) -> crate::types::ClimateProfile {
        room.area_id
            .and_then(|aid| self.get_area_data(&aid).ok().flatten())
            .map(|a| a.climate)
            .unwrap_or_default()
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

    /// Promote legacy per-field bonuses (`hit_bonus`, `damage_bonus`,
    /// `max_hp_bonus`, `max_mana_bonus`, `stat_str..cha`) on every item into
    /// the unified `affects` Vec. Idempotent. Guarded by a single setting key
    /// `item_affects_migration_done` so we don't pay the iteration cost on
    /// every boot. Wired in `main.rs` after the other migrate_* calls.
    pub fn migrate_item_legacy_bonuses_to_affects(&self) -> Result<()> {
        const MIGRATION_KEY: &str = "item_affects_migration_done";
        if let Ok(Some(_)) = self.get_setting(MIGRATION_KEY) {
            return Ok(());
        }
        let mut migrated_count = 0usize;
        let mut scanned_count = 0usize;
        for entry in self.items.iter() {
            scanned_count += 1;
            let (_key, value) = entry?;
            let mut item: ItemData = serde_json::from_slice(&value)?;
            if item.normalize_legacy_bonuses() {
                self.save_item_data(item)?;
                migrated_count += 1;
            }
        }
        let _ = self.set_setting(MIGRATION_KEY, "1");
        if migrated_count > 0 {
            tracing::info!(
                "Migrated legacy bonuses on {migrated_count} of {scanned_count} item(s) into the new affects lane"
            );
        } else if scanned_count > 0 {
            tracing::info!("Item affects migration: nothing to migrate ({scanned_count} item(s) scanned)");
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
        // If the item is equipped, strip any equip-stamped buffs from the
        // wearer first so destroyed items don't leak permanent buffs.
        if let Ok(Some(item)) = self.get_item_data(item_id) {
            let _ = self.strip_item_buffs_from_holder(&item);
        }
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

    /// Per-area entity counts used by the create-time quota gate (F6).
    /// Each one filters the relevant tree by `area_id` (and `is_prototype`
    /// where applicable) and returns the count.
    pub fn count_rooms_in_area(&self, area_id: &Uuid) -> Result<usize> {
        Ok(self
            .list_all_rooms()?
            .into_iter()
            .filter(|r| r.area_id.as_ref() == Some(area_id))
            .count())
    }

    pub fn count_item_protos_in_area(&self, area_id: &Uuid) -> Result<usize> {
        Ok(self
            .list_all_items()?
            .into_iter()
            .filter(|i| i.is_prototype && i.area_id.as_ref() == Some(area_id))
            .count())
    }

    pub fn count_mobile_protos_in_area(&self, area_id: &Uuid) -> Result<usize> {
        Ok(self
            .list_all_mobiles()?
            .into_iter()
            .filter(|m| m.is_prototype && m.area_id.as_ref() == Some(area_id))
            .count())
    }

    pub fn count_spawn_points_in_area(&self, area_id: &Uuid) -> Result<usize> {
        Ok(self
            .list_all_spawn_points()?
            .into_iter()
            .filter(|s| &s.area_id == area_id)
            .count())
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
        item.currently_worn_at = None;
        self.save_item_data(item)?;
        Ok(true)
    }

    /// Move item to a character's inventory
    pub fn move_item_to_inventory(&self, item_id: &Uuid, char_name: &str) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            // If it was equipped on a character or mob, strip equip-stamped buffs first.
            self.strip_item_buffs_from_holder(&item)?;
            item.location = ItemLocation::Inventory(char_name.to_lowercase());
            item.currently_worn_at = None;
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Move item to equipped status on a player. Auto-picks the first free
    /// slot in `item.wear_locations` not already occupied by another
    /// equipped item on this wearer. For deterministic slot selection,
    /// call [`move_item_to_equipped_at`] with an explicit `WearLocation`.
    /// Stamps each `ItemAffect` on the item as a permanent `ActiveBuff`
    /// on the wearer, sourced as `"item:<uuid>"`.
    pub fn move_item_to_equipped(&self, item_id: &Uuid, char_name: &str) -> Result<bool> {
        let slot = match self.get_item_data(item_id)? {
            Some(item) => self.pick_free_wear_slot(char_name, &item)?,
            None => return Ok(false),
        };
        self.move_item_to_equipped_at(item_id, char_name, slot)
    }

    /// Move item to equipped on a player at an explicit slot. Caller is
    /// responsible for ensuring the slot is valid for this item's
    /// `wear_locations`. Slot is persisted on `ItemData.currently_worn_at`
    /// so `%actor.eq(left_hand)%`-style DG accessors can answer
    /// "what's in slot X" queries.
    pub fn move_item_to_equipped_at(
        &self,
        item_id: &Uuid,
        char_name: &str,
        slot: Option<crate::types::WearLocation>,
    ) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            // Strip any stale equip-stamped buffs from the previous holder, then stamp on the new one.
            self.strip_item_buffs_from_holder(&item)?;
            // Defensive: promote any legacy per-field bonuses still on the item.
            if item.normalize_legacy_bonuses() {
                // Field zeroed; will be saved with new location below.
            }
            item.location = ItemLocation::Equipped(char_name.to_lowercase());
            item.currently_worn_at = slot;
            self.stamp_item_buffs_on_character(&item, char_name)?;
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Move item to nowhere (removes from any inventory/location)
    pub fn move_item_to_nowhere(&self, item_id: &Uuid) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            self.strip_item_buffs_from_holder(&item)?;
            item.location = ItemLocation::Nowhere;
            item.currently_worn_at = None;
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Pick the first slot in `item.wear_locations` that isn't already
    /// occupied by another equipped item on the same wearer. Returns
    /// `None` when no free slot exists or the item has no wear locations.
    /// Used as the auto-pick default by [`move_item_to_equipped`].
    pub fn pick_free_wear_slot(
        &self,
        char_name: &str,
        item: &ItemData,
    ) -> Result<Option<crate::types::WearLocation>> {
        if item.wear_locations.is_empty() {
            return Ok(None);
        }
        let already_worn: std::collections::HashSet<crate::types::WearLocation> = self
            .get_equipped_items(char_name)?
            .into_iter()
            .filter(|i| i.id != item.id)
            .filter_map(|i| i.currently_worn_at)
            .collect();
        Ok(item
            .wear_locations
            .iter()
            .copied()
            .find(|slot| !already_worn.contains(slot)))
    }

    /// Mob counterpart to [`pick_free_wear_slot`]. Iterates the mob's
    /// currently equipped items for slot conflict checking.
    pub fn pick_free_wear_slot_for_mobile(
        &self,
        mobile_id: &Uuid,
        item: &ItemData,
    ) -> Result<Option<crate::types::WearLocation>> {
        if item.wear_locations.is_empty() {
            return Ok(None);
        }
        let already_worn: std::collections::HashSet<crate::types::WearLocation> = self
            .get_items_equipped_on_mobile(mobile_id)?
            .into_iter()
            .filter(|i| i.id != item.id)
            .filter_map(|i| i.currently_worn_at)
            .collect();
        Ok(item
            .wear_locations
            .iter()
            .copied()
            .find(|slot| !already_worn.contains(slot)))
    }

    /// Promote any legacy per-field bonuses then push each `ItemAffect` as a
    /// permanent `ActiveBuff` on the character, sourced as `"item:<uuid>"`.
    /// Idempotent: strips any pre-existing buffs with the same source first.
    pub fn stamp_item_buffs_on_character(&self, item: &ItemData, char_name: &str) -> Result<()> {
        if item.affects.is_empty() {
            return Ok(());
        }
        let source = format!("item:{}", item.id);
        let key = char_name.to_lowercase();
        if let Some(mut character) = self.get_character_data(&key)? {
            character.active_buffs.retain(|b| b.source != source);
            for affect in &item.affects {
                character.active_buffs.push(crate::types::ActiveBuff {
                    effect_type: affect.effect_type,
                    magnitude: affect.magnitude,
                    remaining_secs: -1,
                    source: source.clone(),
                    damage_type: affect.damage_type,
                    vs_effect: affect.vs_effect.clone(),
                });
            }
            self.save_character_data(character)?;
        }
        Ok(())
    }

    /// Mob counterpart of `stamp_item_buffs_on_character`.
    pub fn stamp_item_buffs_on_mobile(&self, item: &ItemData, mobile_id: &Uuid) -> Result<()> {
        if item.affects.is_empty() {
            return Ok(());
        }
        let source = format!("item:{}", item.id);
        if let Some(mut mobile) = self.get_mobile_data(mobile_id)? {
            mobile.active_buffs.retain(|b| b.source != source);
            for affect in &item.affects {
                mobile.active_buffs.push(crate::types::ActiveBuff {
                    effect_type: affect.effect_type,
                    magnitude: affect.magnitude,
                    remaining_secs: -1,
                    source: source.clone(),
                    damage_type: affect.damage_type,
                    vs_effect: affect.vs_effect.clone(),
                });
            }
            self.save_mobile_data(mobile)?;
        }
        Ok(())
    }

    /// Strip all `ActiveBuff` entries with `source == "item:<item.id>"` from
    /// whoever currently has the item equipped. No-op if the item is not in an
    /// `Equipped` location.
    pub fn strip_item_buffs_from_holder(&self, item: &ItemData) -> Result<()> {
        let source = format!("item:{}", item.id);
        match &item.location {
            ItemLocation::Equipped(holder) => {
                // Heuristic: a mobile holder is a UUID string; a character holder is a name.
                if let Ok(mob_id) = Uuid::parse_str(holder) {
                    if let Some(mut mobile) = self.get_mobile_data(&mob_id)? {
                        let before = mobile.active_buffs.len();
                        mobile.active_buffs.retain(|b| b.source != source);
                        if mobile.active_buffs.len() != before {
                            self.save_mobile_data(mobile)?;
                        }
                    }
                } else if let Some(mut character) = self.get_character_data(&holder.to_lowercase())? {
                    let before = character.active_buffs.len();
                    character.active_buffs.retain(|b| b.source != source);
                    if character.active_buffs.len() != before {
                        self.save_character_data(character)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
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
        item.currently_worn_at = None;

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
            self.strip_item_buffs_from_holder(&item)?;
            // Use mobile UUID as the owner identifier
            item.location = ItemLocation::Inventory(mobile_id.to_string());
            item.currently_worn_at = None;
            self.save_item_data(item)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Move item to equipped on a mobile (auto-picks the first free
    /// wear slot). For deterministic slot selection (e.g. zone resets
    /// with explicit `SpawnDestination::Equipped(WearLocation)`), call
    /// [`move_item_to_mobile_equipped_at`].
    pub fn move_item_to_mobile_equipped(&self, item_id: &Uuid, mobile_id: &Uuid) -> Result<bool> {
        let slot = match self.get_item_data(item_id)? {
            Some(item) => self.pick_free_wear_slot_for_mobile(mobile_id, &item)?,
            None => return Ok(false),
        };
        self.move_item_to_mobile_equipped_at(item_id, mobile_id, slot)
    }

    /// Move item to equipped on a mobile at an explicit slot. Mob
    /// counterpart of [`move_item_to_equipped_at`].
    pub fn move_item_to_mobile_equipped_at(
        &self,
        item_id: &Uuid,
        mobile_id: &Uuid,
        slot: Option<crate::types::WearLocation>,
    ) -> Result<bool> {
        if let Some(mut item) = self.get_item_data(item_id)? {
            self.strip_item_buffs_from_holder(&item)?;
            if item.normalize_legacy_bonuses() {
                // Field zeroed; will be saved with new location below.
            }
            // Use mobile UUID as the owner identifier
            item.location = ItemLocation::Equipped(mobile_id.to_string());
            item.currently_worn_at = slot;
            self.stamp_item_buffs_on_mobile(&item, mobile_id)?;
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
            // First placement also stamps home_area_id for MOB_STAY_ZONE.
            // Once set, it never moves — wander into another room must not
            // reset the home zone.
            if !mobile.is_prototype && mobile.home_area_id.is_none() {
                if let Some(room) = self.get_room_data(room_id)? {
                    mobile.home_area_id = room.area_id;
                }
            }
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

    // ========== Email Audit Log ==========

    /// Append a row to the bounded email-audit ring. Drops the oldest entries
    /// when the ring exceeds `EMAIL_AUDIT_RING_SIZE`. Errors are surfaced —
    /// the email-send path treats audit-write failure as non-fatal so a
    /// failed audit doesn't block actually delivering the email.
    pub fn record_email_audit(&self, entry: EmailAuditEntry) -> Result<()> {
        // Monotonic 8-byte big-endian key keeps natural sled ordering oldest
        // → newest. Use `generate_id` for atomicity under concurrent writers.
        let id = self.db.generate_id()?;
        let key = id.to_be_bytes();
        let value = serde_json::to_vec(&entry)?;
        self.email_audit.insert(key, value)?;

        // Trim old entries. Cheap: the tree iter is in key order, so we
        // pop fronts until length ≤ ring size. Not exact under concurrent
        // writers, but bounded.
        let mut len = self.email_audit.len();
        while len > EMAIL_AUDIT_RING_SIZE {
            if let Some((k, _)) = self.email_audit.iter().next().transpose()? {
                self.email_audit.remove(k)?;
                len -= 1;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Most recent `limit` audit entries, newest first. Caller decides how
    /// many to surface. Errors propagate; callers commonly fall back to an
    /// empty list rather than refusing the admin command.
    pub fn list_email_audit(&self, limit: usize) -> Result<Vec<EmailAuditEntry>> {
        let mut entries = Vec::new();
        for entry in self.email_audit.iter().rev() {
            if entries.len() >= limit {
                break;
            }
            let (_, v) = entry?;
            if let Ok(parsed) = serde_json::from_slice::<EmailAuditEntry>(&v) {
                entries.push(parsed);
            }
        }
        Ok(entries)
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

    // ========== Class Loadout Functions ==========

    /// Get a class loadout override by class id. Lowercase normalized.
    pub fn get_class_loadout(&self, class_id: &str) -> Result<Option<crate::types::ClassLoadout>> {
        let key = class_id.to_lowercase();
        match self.class_loadouts.get(key.as_bytes())? {
            Some(ivec) => Ok(Some(serde_json::from_slice(&ivec)?)),
            None => Ok(None),
        }
    }

    /// Save (or overwrite) a class loadout override.
    pub fn save_class_loadout(&self, loadout: crate::types::ClassLoadout) -> Result<()> {
        let key = loadout.class_id.to_lowercase();
        let value = serde_json::to_vec(&loadout)?;
        self.class_loadouts.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// Iterate every class loadout override in the tree.
    pub fn list_all_class_loadouts(&self) -> Result<Vec<crate::types::ClassLoadout>> {
        let mut out = Vec::new();
        for entry in self.class_loadouts.iter() {
            let (_key, value) = entry?;
            if let Ok(loadout) = serde_json::from_slice::<crate::types::ClassLoadout>(&value) {
                out.push(loadout);
            }
        }
        Ok(out)
    }

    // ========== Achievement Functions ==========

    /// Get achievement definition by key from the sled tree.
    pub fn get_achievement(&self, key: &str) -> Result<Option<AchievementDef>> {
        let key = key.to_lowercase();
        match self.achievements.get(key.as_bytes())? {
            Some(ivec) => {
                let def: AchievementDef = serde_json::from_slice(&ivec)?;
                Ok(Some(def))
            }
            None => Ok(None),
        }
    }

    /// Save an achievement definition to the sled tree.
    pub fn save_achievement(&self, def: AchievementDef) -> Result<()> {
        let key = def.key.to_lowercase();
        let value = serde_json::to_vec(&def)?;
        self.achievements.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// Delete an achievement definition by key.
    pub fn delete_achievement(&self, key: &str) -> Result<bool> {
        let key = key.to_lowercase();
        Ok(self.achievements.remove(key.as_bytes())?.is_some())
    }

    /// List all achievement definitions in the sled tree.
    pub fn list_all_achievements(&self) -> Result<Vec<AchievementDef>> {
        let mut out = Vec::new();
        for entry in self.achievements.iter() {
            let (_key, value) = entry?;
            let def: AchievementDef = serde_json::from_slice(&value)?;
            out.push(def);
        }
        Ok(out)
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

    /// Delete every mail message addressed to `recipient`. Called from
    /// `delete_character_data` so removing a character also clears their
    /// inbox — otherwise messages orphan in the mail tree forever.
    /// Returns the number of messages removed.
    pub fn delete_mail_for_recipient(&self, recipient: &str) -> Result<usize> {
        let recipient_lower = recipient.to_lowercase();
        let mut to_delete: Vec<Uuid> = Vec::new();
        for entry in self.mail.iter() {
            let (_key, value) = entry?;
            let msg: MailMessage = serde_json::from_slice(&value)?;
            if msg.recipient == recipient_lower {
                to_delete.push(msg.id);
            }
        }
        let mut removed = 0usize;
        for id in &to_delete {
            if self.delete_mail(id)? {
                removed += 1;
            }
        }
        Ok(removed)
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

    // ========== Bulletin Board Functions ==========

    /// Default per-board cap when `ItemData.board_max_messages` is unset.
    /// Matches stock CircleMUD `gen_board.c`.
    pub const DEFAULT_BOARD_MAX_MESSAGES: usize = 60;

    /// Store a new bulletin board post. If the destination board (identified
    /// by `post.board_vnum`) is at or above `max_messages`, the oldest post
    /// is evicted before insertion. Returns `Ok(())` after the insert.
    pub fn store_board_post(&self, post: BoardPost, max_messages: Option<i32>) -> Result<()> {
        let cap = max_messages
            .filter(|n| *n > 0)
            .map(|n| n as usize)
            .unwrap_or(Self::DEFAULT_BOARD_MAX_MESSAGES);
        // Evict oldest until under cap (typically loops zero or one time).
        loop {
            let posts = self.get_board_posts(&post.board_vnum)?;
            if posts.len() < cap {
                break;
            }
            // posts is oldest-first, so [0] is the eviction target.
            let oldest_id = posts[0].id;
            self.delete_board_post(&oldest_id)?;
        }
        let key = post.id.as_bytes();
        let value = serde_json::to_vec(&post)?;
        self.boards.insert(key, value)?;
        Ok(())
    }

    /// Get all posts for a board, sorted oldest-first (stable 1-based
    /// indexing for `board read N`).
    pub fn get_board_posts(&self, board_vnum: &str) -> Result<Vec<BoardPost>> {
        let mut posts = Vec::new();
        for entry in self.boards.iter() {
            let (_key, value) = entry?;
            let post: BoardPost = serde_json::from_slice(&value)?;
            if post.board_vnum == board_vnum {
                posts.push(post);
            }
        }
        posts.sort_by_key(|p| p.posted_at);
        Ok(posts)
    }

    /// Delete a single post by id. Returns true if a post was removed.
    pub fn delete_board_post(&self, id: &Uuid) -> Result<bool> {
        let key = id.as_bytes();
        Ok(self.boards.remove(key)?.is_some())
    }

    /// Count posts on a board.
    pub fn count_board_posts(&self, board_vnum: &str) -> Result<usize> {
        let mut n = 0usize;
        for entry in self.boards.iter() {
            let (_key, value) = entry?;
            let post: BoardPost = serde_json::from_slice(&value)?;
            if post.board_vnum == board_vnum {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Delete every post authored by `name` across all boards. Called from
    /// `delete_character_data` to keep the boards tree from accumulating
    /// orphan posts after character deletion. Returns count removed.
    pub fn delete_board_posts_by_author(&self, name: &str) -> Result<usize> {
        let lower = name.to_lowercase();
        let mut to_delete: Vec<Uuid> = Vec::new();
        for entry in self.boards.iter() {
            let (_key, value) = entry?;
            let post: BoardPost = serde_json::from_slice(&value)?;
            if post.author.to_lowercase() == lower {
                to_delete.push(post.id);
            }
        }
        let mut removed = 0usize;
        for id in &to_delete {
            if self.delete_board_post(id)? {
                removed += 1;
            }
        }
        Ok(removed)
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
        _temp: tempfile::TempDir,
    }
    fn open_temp(_tag: &str) -> TempDb {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = Db::open(temp.path()).expect("open db");
        TempDb { db, _temp: temp }
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
    fn email_audit_ring_trims_to_max_size() {
        let t = open_temp("audit_trim");
        // Insert one over the cap and confirm the oldest entry is dropped.
        for i in 0..=EMAIL_AUDIT_RING_SIZE {
            t.db.record_email_audit(EmailAuditEntry {
                timestamp: i as i64,
                kind: "verification".into(),
                account_name: format!("user{}", i),
                outcome: "sent".into(),
            })
            .expect("record audit");
        }
        let entries = t.db.list_email_audit(EMAIL_AUDIT_RING_SIZE * 2).unwrap();
        assert_eq!(entries.len(), EMAIL_AUDIT_RING_SIZE);
        // Newest first — the very latest insert (i = RING_SIZE) is at index 0.
        assert_eq!(entries[0].timestamp, EMAIL_AUDIT_RING_SIZE as i64);
        // Oldest survivor is the second insert (i = 1) since i = 0 fell out.
        assert_eq!(entries.last().unwrap().timestamp, 1);
    }

    #[test]
    fn email_audit_returns_newest_first_capped_to_limit() {
        let t = open_temp("audit_limit");
        for i in 0..5 {
            t.db.record_email_audit(EmailAuditEntry {
                timestamp: i,
                kind: "reset".into(),
                account_name: "alice".into(),
                outcome: "sent".into(),
            })
            .unwrap();
        }
        let entries = t.db.list_email_audit(3).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].timestamp, 4);
        assert_eq!(entries[2].timestamp, 2);
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

    #[test]
    fn hash_password_rejects_oversized_input() {
        let t = open_temp("pw_long");
        let oversized = "a".repeat(MAX_PASSWORD_LEN + 1);
        assert!(t.db.hash_password(&oversized).is_err());
        // Boundary: exactly the cap is accepted.
        let at_cap = "a".repeat(MAX_PASSWORD_LEN);
        assert!(t.db.hash_password(&at_cap).is_ok());
    }

    #[test]
    fn verify_password_short_circuits_oversized_input() {
        let t = open_temp("pw_verify_long");
        let hash = t.db.hash_password("hunter2").expect("hash");
        let oversized = "a".repeat(MAX_PASSWORD_LEN + 1);
        // Must return Ok(false) without invoking Argon2 — no panic, no error.
        assert_eq!(t.db.verify_password(&oversized, &hash).unwrap(), false);
        // Sanity: the real password still verifies.
        assert_eq!(t.db.verify_password("hunter2", &hash).unwrap(), true);
    }
}
