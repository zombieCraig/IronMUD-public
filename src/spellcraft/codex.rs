//! Codex administration: the Ancient seed file and moderation ops.
//!
//! Every op writes the `codex` tree first (CAS) and then mirrors the
//! definition into `World.spell_definitions`, so a change is live at once.

use std::collections::HashMap;

use crate::SharedState;
use crate::db::Db;
use crate::types::{CODEX_ID_PREFIX, CanonStatus, CodexEntry, SpellDefinition, SpellOrigin};

use super::power::ALLOWED_PLAYER_TYPES;
use super::text::{sanitize_description, sanitize_flavor, validate_spell_name};

/// Where seeded world-history spells live. Ships empty.
pub const ANCIENT_SEED_PATH: &str = "scripts/data/spells_ancient.json";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SeedReport {
    pub added: Vec<String>,
    /// Ids already in the codex (seeding never overwrites).
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl SeedReport {
    pub fn summary(&self) -> String {
        let mut s = format!("{} added, {} already present", self.added.len(), self.skipped);
        if !self.errors.is_empty() {
            s.push_str(&format!(", {} rejected:", self.errors.len()));
            for e in &self.errors {
                s.push_str("\n  ");
                s.push_str(e);
            }
        }
        s
    }
}

/// Parse the seed document: a JSON array of `{def, meta}`, or an object
/// whose `spells` key holds that array (so the file can carry `_doc`).
fn parse_seed(content: &str) -> Result<Vec<serde_json::Value>, String> {
    let doc: serde_json::Value = serde_json::from_str(content).map_err(|e| format!("bad JSON: {e}"))?;
    match doc {
        serde_json::Value::Array(a) => Ok(a),
        serde_json::Value::Object(mut o) => match o.remove("spells") {
            Some(serde_json::Value::Array(a)) => Ok(a),
            None => Ok(Vec::new()),
            Some(_) => Err("`spells` must be an array".to_string()),
        },
        _ => Err("expected an array of {def, meta}".to_string()),
    }
}

/// Check and normalise one seed entry. Player text is sanitised exactly as
/// it will be for authored spells.
fn prepare(mut entry: CodexEntry, spells: &HashMap<String, SpellDefinition>) -> Result<CodexEntry, String> {
    let id = entry.def.id.clone();
    if !id.starts_with(CODEX_ID_PREFIX) {
        return Err(format!("{id}: codex ids must start with '{CODEX_ID_PREFIX}'"));
    }
    if !ALLOWED_PLAYER_TYPES.contains(&entry.def.spell_type.as_str()) {
        return Err(format!(
            "{id}: spell_type '{}' is not one of {}",
            entry.def.spell_type,
            ALLOWED_PLAYER_TYPES.join("/")
        ));
    }
    if entry.meta.god_vnum.is_empty() {
        return Err(format!("{id}: meta.god_vnum is required"));
    }
    validate_spell_name(&entry.def.name, spells, Some(&id)).map_err(|e| format!("{id}: {}", e.message()))?;
    entry.def.description = sanitize_description(&entry.def.description);
    entry.meta.flavor.caster = sanitize_flavor(&entry.meta.flavor.caster);
    entry.meta.flavor.room = sanitize_flavor(&entry.meta.flavor.room);
    entry.meta.flavor.target = sanitize_flavor(&entry.meta.flavor.target);
    entry.meta.origin = SpellOrigin::Ancient;
    entry.def.codex = true;
    Ok(entry)
}

/// Insert every new entry of a seed document into the codex and into
/// `spells`. Never overwrites an existing codex id.
pub fn seed_from_str(db: &Db, spells: &mut HashMap<String, SpellDefinition>, content: &str) -> SeedReport {
    let mut report = SeedReport::default();
    let values = match parse_seed(content) {
        Ok(v) => v,
        Err(e) => {
            report.errors.push(e);
            return report;
        }
    };
    for (i, value) in values.into_iter().enumerate() {
        let entry: CodexEntry = match serde_json::from_value(value) {
            Ok(e) => e,
            Err(e) => {
                report.errors.push(format!("entry {i}: {e}"));
                continue;
            }
        };
        if matches!(db.get_codex(&entry.def.id), Ok(Some(_))) {
            report.skipped += 1;
            continue;
        }
        let entry = match prepare(entry, spells) {
            Ok(e) => e,
            Err(e) => {
                report.errors.push(e);
                continue;
            }
        };
        match db.insert_codex_if_absent(&entry) {
            Ok(true) => {
                spells.insert(entry.def.id.clone(), super::world_def(&entry));
                report.added.push(entry.def.id.clone());
            }
            Ok(false) => report.skipped += 1,
            Err(e) => report.errors.push(format!("{}: {e}", entry.def.id)),
        }
    }
    report
}

/// Seed from [`ANCIENT_SEED_PATH`]. A missing file is not an error.
pub fn seed_ancient(db: &Db, spells: &mut HashMap<String, SpellDefinition>) -> SeedReport {
    match std::fs::read_to_string(ANCIENT_SEED_PATH) {
        Ok(content) => seed_from_str(db, spells, &content),
        Err(_) => SeedReport::default(),
    }
}

// ========== Moderation ==========

fn store_and_mirror<F>(db: &Db, state: &SharedState, id: &str, f: F) -> Result<CodexEntry, String>
where
    F: FnMut(&mut CodexEntry),
{
    match db.update_codex(id, f) {
        Ok(Some(entry)) => {
            super::mirror_into_world(state, &entry);
            Ok(entry)
        }
        Ok(None) => Err(format!("No codex spell '{id}'.")),
        Err(e) => Err(format!("Codex write failed: {e}")),
    }
}

pub fn parse_status(s: &str) -> Option<CanonStatus> {
    match s.to_ascii_lowercase().as_str() {
        "canon" => Some(CanonStatus::Canon),
        "faded" => Some(CanonStatus::Faded),
        "lost" => Some(CanonStatus::Lost),
        _ => None,
    }
}

pub fn status_str(s: CanonStatus) -> &'static str {
    match s {
        CanonStatus::Canon => "canon",
        CanonStatus::Faded => "faded",
        CanonStatus::Lost => "lost",
    }
}

pub fn set_status(db: &Db, state: &SharedState, id: &str, status: CanonStatus) -> Result<CodexEntry, String> {
    store_and_mirror(db, state, id, |e| e.meta.status = status)
}

/// Veto: Faded and hidden from player listings. Lifting a veto leaves the
/// spell Faded; restore its status separately.
pub fn set_veto(db: &Db, state: &SharedState, id: &str, vetoed: bool) -> Result<CodexEntry, String> {
    store_and_mirror(db, state, id, |e| {
        e.meta.vetoed = vetoed;
        if vetoed {
            e.meta.status = CanonStatus::Faded;
        }
    })
}

/// The author renounced the god: the spell stays in the canon, but its
/// author is named Faithless and loses the full form.
pub fn set_disgraced(db: &Db, state: &SharedState, id: &str, disgraced: bool) -> Result<CodexEntry, String> {
    store_and_mirror(db, state, id, |e| e.meta.author_disgraced = disgraced)
}

pub fn rename(db: &Db, state: &SharedState, id: &str, name: &str) -> Result<CodexEntry, String> {
    // World read happens before the codex write; nothing here holds both.
    let check = {
        let world = state.lock();
        validate_spell_name(name, &world.spell_definitions, Some(id))
    };
    check.map_err(|e| e.message())?;
    let name = name.to_string();
    store_and_mirror(db, state, id, |e| {
        e.def.name = name.clone();
        e.meta.version += 1;
    })
}
