//! CircleMUD-style social command registry and player-side dispatcher.
//!
//! [`SocialRegistry`] owns the loaded table of [`SocialAction`]s and is
//! keyed by both the primary command name and its abbrev so a verb like
//! `wave` and its `wav` shortcut point at the same record. The actual
//! 1.2 KB JSON lives at `scripts/data/socials.json` (produced by the
//! CircleMUD importer in `src/import/engines/circle/socials.rs`).
//!
//! [`dispatch_player_social`] is the Rust-side handler invoked from the
//! command loop in `src/lib.rs`. It bypasses the Rhai dispatch for
//! socials so we don't pay engine-call overhead for ~490 verbs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tracing::warn;
use uuid::Uuid;

use crate::types::{CharacterData, SocialAction, SocialPosition};
use crate::{SharedConnections, SharedState};

use super::render::{self, RenderObject, RenderParty};

/// On-disk format for `scripts/data/socials.json` — a thin wrapper so we
/// can add metadata (version, source path, importer revision) without
/// breaking older consumers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SocialsFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub socials: Vec<SocialAction>,
}

/// Loaded, indexed socials table. Cheap to clone (everything sits behind
/// `Arc`).
#[derive(Debug, Default, Clone)]
pub struct SocialRegistry {
    by_name: HashMap<String, Arc<SocialAction>>,
    /// Canonical primary entries (deduplicated). Iteration order matches
    /// insertion order from the JSON file so help listings come out
    /// alphabetical when the file is sorted.
    canonical: Vec<Arc<SocialAction>>,
}

/// Process-wide registry handle. Initialized lazily on first access so
/// background ticks (simulation, NPC ambient emotes) can read the table
/// without taking the World lock. The startup path in `main.rs` calls
/// [`init_global`] explicitly so the same `Arc<SocialRegistry>` lives
/// in both `World.socials` and this handle.
static GLOBAL_REGISTRY: OnceLock<Arc<SocialRegistry>> = OnceLock::new();

/// Install a freshly-loaded registry as the process-wide singleton.
/// Idempotent: a second call returns the originally-stored registry.
pub fn init_global(reg: SocialRegistry) -> Arc<SocialRegistry> {
    let arc = Arc::new(reg);
    GLOBAL_REGISTRY.get_or_init(|| arc.clone()).clone()
}

/// Get the process-wide registry, loading from disk on first call if
/// startup hasn't installed one. Returns an empty registry if the JSON
/// is absent — sim ticks fall back to their hand-rolled emote list.
pub fn registry() -> Arc<SocialRegistry> {
    GLOBAL_REGISTRY
        .get_or_init(|| Arc::new(SocialRegistry::load_path("scripts/data/socials.json")))
        .clone()
}

impl SocialRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_actions(items: Vec<SocialAction>) -> Self {
        let mut reg = SocialRegistry::default();
        for action in items {
            reg.insert(action);
        }
        auto_tag(&mut reg);
        reg
    }

    /// Load from a serialized [`SocialsFile`] JSON blob. Bare arrays (the
    /// raw `Vec<SocialAction>` shape) are also accepted for forward
    /// compatibility with hand-authored files.
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        if let Ok(file) = serde_json::from_str::<SocialsFile>(s) {
            return Ok(SocialRegistry::from_actions(file.socials));
        }
        let bare: Vec<SocialAction> = serde_json::from_str(s)?;
        Ok(SocialRegistry::from_actions(bare))
    }

    /// Load from a file path. Missing file resolves to an empty registry
    /// (with a warning) so a dev who hasn't run the importer yet can
    /// still boot the server.
    pub fn load_path(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(s) => match SocialRegistry::from_json(&s) {
                Ok(reg) => reg,
                Err(e) => {
                    warn!(
                        "Failed to parse socials JSON at {}: {}. Continuing with empty registry.",
                        path.display(),
                        e
                    );
                    SocialRegistry::default()
                }
            },
            Err(e) => {
                warn!(
                    "Socials file {} not found ({}). Continuing with empty registry — run `ironmud-import circle <tba-lib>` to populate.",
                    path.display(),
                    e
                );
                SocialRegistry::default()
            }
        }
    }

    /// Insert a single social. Both the primary name and the abbrev key
    /// into the same `Arc`. Duplicate names overwrite (last writer wins).
    pub fn insert(&mut self, action: SocialAction) {
        let arc = Arc::new(action);
        let key = arc.lookup_key();
        // Track canonical-list membership by name so reloads don't bloat
        // the Vec when the same JSON is read twice.
        if self
            .canonical
            .iter()
            .position(|a| a.lookup_key() == key)
            .is_none()
        {
            self.canonical.push(arc.clone());
        }
        self.by_name.insert(key.clone(), arc.clone());
        if let Some(abbrev) = &arc.abbrev {
            let alias = abbrev.to_ascii_lowercase();
            // Don't let an abbrev shadow a primary command name from a
            // different social — primary names always win.
            if !self.by_name.contains_key(&alias) || self.by_name[&alias].lookup_key() == key {
                self.by_name.insert(alias, arc.clone());
            }
        }
    }

    /// Look up a social by name or abbrev (case-insensitive).
    pub fn get(&self, verb: &str) -> Option<&Arc<SocialAction>> {
        self.by_name.get(&verb.to_ascii_lowercase())
    }

    /// Iterate canonical primary entries (no abbrev duplicates).
    pub fn iter(&self) -> impl Iterator<Item = &Arc<SocialAction>> {
        self.canonical.iter()
    }

    pub fn len(&self) -> usize {
        self.canonical.len()
    }

    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
    }

    /// All canonical socials carrying any of the requested tags. Used by
    /// the NPC sim tick to draw an emote that matches a mobile's
    /// current mood/needs.
    pub fn tagged_any(&self, want: &[crate::types::SocialTag]) -> Vec<Arc<SocialAction>> {
        self.canonical
            .iter()
            .filter(|a| a.tags.iter().any(|t| want.contains(t)))
            .cloned()
            .collect()
    }
}

/// Annotate well-known imported socials with sim-relevant [`SocialTag`]s.
/// The CircleMUD `socials.new` format carries no tag data, so we layer
/// our own classification on top at load time. Untagged socials are
/// still fully usable by players, DG triggers, and tab completion —
/// they just don't surface in NPC ambient emote selection.
fn auto_tag(reg: &mut SocialRegistry) {
    use crate::types::SocialTag::*;
    // (name, tags). Each canonical entry is matched case-insensitively.
    const KNOWN: &[(&str, &[crate::types::SocialTag])] = &[
        // Sad / depressed / breakdown spectrum.
        ("sigh", &[Sad, Depressed]),
        ("frown", &[Sad]),
        ("pout", &[Sad]),
        ("sniff", &[Sad]),
        ("sob", &[Depressed, Breakdown]),
        ("weep", &[Depressed, Breakdown, Grief]),
        ("cry", &[Breakdown, Grief]),
        ("mourn", &[Grief]),
        ("scream", &[Breakdown]),
        ("tremble", &[Breakdown]),
        ("mope", &[Sad, Depressed]),
        ("wail", &[Breakdown, Grief]),
        // Needs cues.
        ("groan", &[Hungry, Uncomfortable]),
        ("growl", &[Hungry, Aggression]),
        ("yawn", &[Tired]),
        ("stretch", &[Tired, Idle]),
        ("snore", &[Tired]),
        // Idle / fidgety.
        ("fidget", &[Idle, Uncomfortable]),
        ("shrug", &[Idle]),
        ("stare", &[Idle]),
        ("whistle", &[Idle, Content]),
        // Content.
        ("smile", &[Content, Greeting]),
        ("grin", &[Content]),
        ("beam", &[Content]),
        ("hum", &[Content, Idle]),
        ("laugh", &[Content]),
        ("chuckle", &[Content]),
        // Greeting / farewell.
        ("wave", &[Greeting, Farewell]),
        ("bow", &[Greeting]),
        ("nod", &[Greeting]),
        ("salute", &[Greeting]),
        ("greet", &[Greeting]),
        ("curtsey", &[Greeting]),
        // Affection (used for `Comfort` toward bereaved/sad partners).
        ("hug", &[Affection, Comfort]),
        ("pat", &[Affection, Comfort]),
        ("kiss", &[Affection]),
        ("cuddle", &[Affection]),
        ("comfort", &[Comfort]),
        ("console", &[Comfort, Grief]),
        // Aggression / discomfort.
        ("glare", &[Aggression, Uncomfortable]),
        ("fume", &[Aggression, Uncomfortable]),
        ("sulk", &[Sad, Uncomfortable]),
        ("snarl", &[Aggression]),
        ("growl", &[Aggression]),
    ];
    for (name, tags) in KNOWN {
        let key = name.to_ascii_lowercase();
        // Need to find the existing Arc, replace its inner with a tagged
        // clone. `Arc::make_mut` would copy-on-write but the Arc has
        // multiple keys (name + abbrev), so we rebuild and reinsert.
        let existing = match reg.by_name.get(&key) {
            Some(a) => a.clone(),
            None => continue,
        };
        // Skip if tags already populated — manual JSON authoring wins.
        if !existing.tags.is_empty() {
            continue;
        }
        let mut new_action = (*existing).clone();
        new_action.tags = tags.to_vec();
        // Re-insert. `insert` indexes both name and abbrev again.
        // Remove the abbrev entry first so the rebuild doesn't double-
        // count it in `canonical`.
        if let Some(ab) = &existing.abbrev {
            reg.by_name.remove(&ab.to_ascii_lowercase());
        }
        reg.canonical.retain(|a| a.lookup_key() != key);
        reg.by_name.remove(&key);
        reg.insert(new_action);
    }
}

/// Outcome of a player social dispatch, used by callers that need to
/// know whether to suppress the normal command lookup.
#[derive(Debug)]
pub enum DispatchOutcome {
    /// Verb wasn't a social. Caller should continue with normal Rhai
    /// dispatch.
    NotASocial,
    /// Social was matched and processed (success or in-game refusal).
    Handled,
}

/// Try to dispatch `verb` as a CircleMUD-style social. Returns
/// [`DispatchOutcome::NotASocial`] if the registry has no such verb;
/// otherwise the social is fully handled and the caller skips its
/// normal dispatch path.
///
/// This is the player command entry point only. NPC sim emotes call
/// [`render::render`] directly with their own broadcast plumbing, and
/// DG triggers go through [`crate::script::dg`].
pub fn dispatch_player_social(
    state: &SharedState,
    connections: &SharedConnections,
    connection_id: uuid::Uuid,
    verb: &str,
    args: &str,
) -> DispatchOutcome {
    // Resolve the social under a brief World lock, then drop it before
    // touching connections — matches the deadlock-prevention rule in
    // CLAUDE.md (never hold World + Connections simultaneously).
    let social = {
        let world = state.lock().unwrap();
        match world.socials.get(verb) {
            Some(s) => s.clone(),
            None => return DispatchOutcome::NotASocial,
        }
    };

    // Snapshot actor state from connections.
    let actor_snapshot = {
        let conns = connections.lock().unwrap();
        conns
            .get(&connection_id)
            .and_then(|s| s.character.as_ref().cloned())
    };
    let Some(actor) = actor_snapshot else {
        // Logged-out caller — the lib.rs access check should have
        // refused, but stay safe.
        send_to(connections, connection_id, "You must be logged in to use socials.\n");
        return DispatchOutcome::Handled;
    };

    // Position gate.
    let actor_pos = SocialPosition::from_character(actor.position);
    if !SocialPosition::permits(social.min_char_position, actor_pos) {
        let msg = format!(
            "You're too {} to do that.\n",
            actor_pos.to_display_string()
        );
        send_to(connections, connection_id, &msg);
        return DispatchOutcome::Handled;
    }

    let actor_party = RenderParty {
        visible_name: &actor.name,
        gender: render::parse_gender(&actor.gender),
    };

    let trimmed = args.trim();

    // Empty-arg branch.
    if trimmed.is_empty() {
        emit_no_arg(connections, &social, &actor, &actor_party);
        return DispatchOutcome::Handled;
    }

    // Self-target shortcut: "smile self" / "smile me".
    if matches!(trimmed.to_ascii_lowercase().as_str(), "self" | "me") {
        emit_self(connections, &social, &actor, &actor_party);
        return DispatchOutcome::Handled;
    }

    // Find a target — try players in room first (case-insensitive name
    // prefix), then mobiles by keyword.
    let target = resolve_target(state, connections, &actor, trimmed);
    match target {
        Some(Target::Player(t)) => {
            // Victim position gate.
            let vict_pos = SocialPosition::from_character(t.position);
            if !SocialPosition::permits(social.min_victim_position, vict_pos) {
                let msg = format!(
                    "{} is too {} for that.\n",
                    t.name,
                    vict_pos.to_display_string()
                );
                send_to(connections, connection_id, &msg);
                return DispatchOutcome::Handled;
            }
            let vict_party = RenderParty {
                visible_name: &t.name,
                gender: render::parse_gender(&t.gender),
            };
            emit_found(
                connections,
                &social,
                &actor,
                &actor_party,
                &vict_party,
                Some(&t.name),
            );
        }
        Some(Target::Mobile { name, gender }) => {
            let vict_party = RenderParty {
                visible_name: &name,
                gender: render::parse_gender(&gender),
            };
            // Mobs don't currently expose a player-style position field
            // to the socials gate — assume they pass (the simulation
            // tick already keeps mobs in plausible positions for their
            // activity).
            emit_found(connections, &social, &actor, &actor_party, &vict_party, None);
        }
        None => {
            let msg = social
                .not_found
                .as_deref()
                .map(|t| render::render(t, &actor_party, None, None, None))
                .unwrap_or_else(|| "You don't see them here.".to_string());
            send_to(connections, connection_id, &format!("{}\n", msg));
        }
    }

    DispatchOutcome::Handled
}

enum Target {
    Player(CharacterData),
    Mobile { name: String, gender: String },
}

fn resolve_target(
    state: &SharedState,
    connections: &SharedConnections,
    actor: &CharacterData,
    keyword: &str,
) -> Option<Target> {
    let keyword_l = keyword.to_ascii_lowercase();
    // Players in the same room (skip the actor themselves).
    {
        let conns = connections.lock().unwrap();
        for (_, sess) in conns.iter() {
            if let Some(ch) = &sess.character {
                if ch.current_room_id != actor.current_room_id {
                    continue;
                }
                if ch.name.eq_ignore_ascii_case(&actor.name) {
                    continue;
                }
                if ch.name.to_ascii_lowercase().starts_with(&keyword_l) {
                    return Some(Target::Player(ch.clone()));
                }
            }
        }
    }
    // Mobiles in the room.
    let db = {
        let world = state.lock().unwrap();
        world.db.clone()
    };
    if let Ok(mobs) = db.get_mobiles_in_room(&actor.current_room_id) {
        for m in mobs {
            let name_match = m.short_desc.to_ascii_lowercase().contains(&keyword_l)
                || m.keywords
                    .iter()
                    .any(|k| k.to_ascii_lowercase().starts_with(&keyword_l));
            if name_match {
                let gender = m
                    .characteristics
                    .as_ref()
                    .map(|c| c.gender.clone())
                    .unwrap_or_default();
                return Some(Target::Mobile {
                    name: m.short_desc.clone(),
                    gender,
                });
            }
        }
    }
    None
}

fn emit_no_arg(
    connections: &SharedConnections,
    social: &SocialAction,
    actor: &CharacterData,
    actor_party: &RenderParty<'_>,
) {
    if let Some(t) = &social.char_no_arg {
        let line = render::render(t, actor_party, None, None, None);
        send_to_actor(connections, actor, &line);
    }
    if let (false, Some(t)) = (social.hide, &social.others_no_arg) {
        let line = render::render(t, actor_party, None, None, None);
        broadcast_awake_with_color(connections, actor.current_room_id, &line, &actor.name);
    }
}

fn emit_self(
    connections: &SharedConnections,
    social: &SocialAction,
    actor: &CharacterData,
    actor_party: &RenderParty<'_>,
) {
    if let Some(t) = &social.char_auto {
        let line = render::render(t, actor_party, None, None, None);
        send_to_actor(connections, actor, &line);
    }
    if let (false, Some(t)) = (social.hide, &social.others_auto) {
        let line = render::render(t, actor_party, None, None, None);
        broadcast_awake_with_color(connections, actor.current_room_id, &line, &actor.name);
    }
}

fn emit_found(
    connections: &SharedConnections,
    social: &SocialAction,
    actor: &CharacterData,
    actor_party: &RenderParty<'_>,
    vict_party: &RenderParty<'_>,
    victim_player_name: Option<&str>,
) {
    let object: Option<&RenderObject<'_>> = None;
    // Actor sees char_found.
    if let Some(t) = &social.char_found {
        let line = render::render(t, actor_party, Some(vict_party), object, None);
        send_to_actor(connections, actor, &line);
    }
    // Victim sees vict_found (if a player).
    if let (Some(name), Some(t)) = (victim_player_name, &social.vict_found) {
        let line = render::render(t, actor_party, Some(vict_party), object, None);
        send_to_player_named(connections, name, &line);
    }
    // Room sees others_found.
    if let Some(t) = &social.others_found {
        let line = render::render(t, actor_party, Some(vict_party), object, None);
        if social.hide {
            // Hide mode: do not broadcast to the room at all.
            return;
        }
        // Exclude actor + victim from the room broadcast since they
        // already got their own variant.
        broadcast_excluding_two(
            connections,
            actor.current_room_id,
            &line,
            &actor.name,
            victim_player_name,
        );
    }
}

/// Send to a specific connection by id. Translates inline tba color
/// codes to ANSI (or strips them) based on the recipient's setting.
fn send_to(connections: &SharedConnections, connection_id: Uuid, msg: &str) {
    let conns = connections.lock().unwrap();
    if let Some(s) = conns.get(&connection_id) {
        let rendered = render::apply_tba_color_codes(msg, s.colors_enabled);
        let _ = s.sender.send(rendered);
    }
}

/// Send to whichever connection currently owns the actor's character.
fn send_to_actor(connections: &SharedConnections, actor: &CharacterData, msg: &str) {
    let conns = connections.lock().unwrap();
    for (_, sess) in conns.iter() {
        if let Some(ch) = &sess.character {
            if ch.name == actor.name {
                let rendered = render::apply_tba_color_codes(msg, sess.colors_enabled);
                let _ = sess.sender.send(format!("{}\n", rendered));
                return;
            }
        }
    }
}

fn send_to_player_named(connections: &SharedConnections, name: &str, msg: &str) {
    let conns = connections.lock().unwrap();
    for (_, sess) in conns.iter() {
        if let Some(ch) = &sess.character {
            if ch.name.eq_ignore_ascii_case(name) {
                let rendered = render::apply_tba_color_codes(msg, sess.colors_enabled);
                let _ = sess.sender.send(format!("{}\n", rendered));
                return;
            }
        }
    }
}

fn broadcast_excluding_two(
    connections: &SharedConnections,
    room_id: Uuid,
    msg: &str,
    actor_name: &str,
    victim_name: Option<&str>,
) {
    // broadcast_to_room_awake already excludes a single name; layer the
    // victim exclusion here. We also iterate per-recipient so each player
    // gets tba color codes translated (or stripped) to match their setting.
    let conns = connections.lock().unwrap();
    for (_, sess) in conns.iter() {
        if let Some(ch) = &sess.character {
            if ch.current_room_id != room_id {
                continue;
            }
            if ch.name == actor_name {
                continue;
            }
            if let Some(v) = victim_name {
                if ch.name.eq_ignore_ascii_case(v) {
                    continue;
                }
            }
            if matches!(
                ch.position,
                crate::types::CharacterPosition::Sleeping
            ) {
                continue;
            }
            let rendered = render::apply_tba_color_codes(msg, sess.colors_enabled);
            let _ = sess.sender.send(format!("{}\n", rendered));
        }
    }
}

/// Per-recipient awake-only room broadcast that translates tba color
/// codes for each listener. Used by `emit_no_arg` and `emit_self`.
fn broadcast_awake_with_color(
    connections: &SharedConnections,
    room_id: Uuid,
    msg: &str,
    exclude_name: &str,
) {
    let conns = connections.lock().unwrap();
    for (_, sess) in conns.iter() {
        if let Some(ch) = &sess.character {
            if ch.current_room_id != room_id {
                continue;
            }
            if ch.name == exclude_name {
                continue;
            }
            if matches!(
                ch.position,
                crate::types::CharacterPosition::Sleeping
            ) {
                continue;
            }
            if ch.ignored.iter().any(|i| i == exclude_name) {
                continue;
            }
            let rendered = render::apply_tba_color_codes(msg, sess.colors_enabled);
            let _ = sess.sender.send(format!("{}\n", rendered));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SocialAction;

    fn smile() -> SocialAction {
        SocialAction {
            name: "smile".to_string(),
            abbrev: Some("smi".to_string()),
            hide: false,
            min_victim_position: SocialPosition::Sleeping,
            min_char_position: SocialPosition::Sleeping,
            min_level: 0,
            char_no_arg: Some("You smile happily.".into()),
            others_no_arg: Some("$n smiles happily.".into()),
            char_found: Some("You smile at $N.".into()),
            others_found: Some("$n smiles at $N.".into()),
            vict_found: Some("$n smiles at you.".into()),
            not_found: Some("You don't see them here.".into()),
            char_auto: Some("You smile at yourself.".into()),
            others_auto: Some("$n smiles at $mself.".into()),
            body_char_found: None,
            body_others_found: None,
            body_vict_found: None,
            object_char_found: None,
            object_others_found: None,
            tags: vec![],
        }
    }

    #[test]
    fn registry_indexes_name_and_abbrev() {
        let reg = SocialRegistry::from_actions(vec![smile()]);
        assert!(reg.get("smile").is_some());
        assert!(reg.get("SMILE").is_some());
        assert!(reg.get("smi").is_some());
        assert!(reg.get("wave").is_none());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_canonical_dedupes() {
        let mut reg = SocialRegistry::new();
        reg.insert(smile());
        reg.insert(smile()); // same name → overwrite, no canonical bloat
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn loads_stock_socials_json() {
        // Smoke test against the committed `scripts/data/socials.json`
        // produced by the tbamud importer. Skips silently if the file
        // is absent (fresh checkouts without a populated table).
        let path = std::path::Path::new("scripts/data/socials.json");
        if !path.exists() {
            return;
        }
        let reg = SocialRegistry::load_path(path);
        assert!(
            reg.len() >= 100,
            "expected at least 100 socials, got {}",
            reg.len()
        );
        // A handful of well-known names should always be present.
        // tbamud uses plural forms like `nods` and `waves` (with `nod`
        // appearing as an abbrev of `nods`); the dispatcher resolves
        // either via the same lookup so we accept both shapes.
        for name in ["smile", "bow", "grin", "sigh"] {
            assert!(
                reg.get(name).is_some(),
                "stock socials.json missing `{}`",
                name
            );
        }
        // Verify abbrev lookup resolves the plural tbamud canonical
        // names: `nod` is the abbrev for `nods`.
        let nod = reg.get("nod").expect("`nod` should resolve via abbrev");
        assert!(
            nod.name == "nod" || nod.name == "nods",
            "unexpected canonical name for nod: {}",
            nod.name
        );
        // Auto-tagging should have stamped `Sad` onto `sigh` (which has
        // empty tags in the JSON).
        let sigh = reg.get("sigh").expect("sigh exists");
        assert!(
            sigh.tags.iter().any(|t| matches!(t, crate::types::SocialTag::Sad)),
            "auto_tag should have tagged `sigh` with Sad"
        );
    }

    #[test]
    fn from_json_accepts_wrapper_and_bare_array() {
        let bare = r#"[{
            "name": "wave",
            "char_no_arg": "You wave.",
            "others_no_arg": "$n waves."
        }]"#;
        let reg = SocialRegistry::from_json(bare).unwrap();
        assert!(reg.get("wave").is_some());

        let wrapped = r#"{
            "source": "test",
            "socials": [{ "name": "bow", "char_no_arg": "You bow." }]
        }"#;
        let reg = SocialRegistry::from_json(wrapped).unwrap();
        assert!(reg.get("bow").is_some());
    }
}
