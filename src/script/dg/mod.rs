//! DG Scripts interpreter — runtime evaluator for the trigger language used
//! by tbamud and circlemud, rehosted in Rust against IronMUD's world API.
//!
//! ## Architecture
//!
//! - [`parser`] turns a `.trg` body string into a [`ast::Block`] (a `Vec<Stmt>`).
//! - [`eval`] tree-walks the AST against an [`EvalCtx`], returning an [`Outcome`].
//! - [`vars`] resolves `%actor.field%`-style substitutions against the ctx.
//! - [`cmds`] dispatches DG commands (`%send%`, `mecho`, `mload`, …) to
//!   IronMUD's existing world API (broadcast, spawn, damage, teleport, …).
//!
//! ## Phase 1 scope (landed)
//!
//! Implemented: `if/elseif/else/end`, `switch/case/default/break/done`,
//! `set`, `eval` (string ops only), `halt`, `return`, comments.
//! Variables: `%actor.field%`, `%self.field%`, `%arg%`, `%cmd%`, `%random.N%`.
//! Commands: `%send%`/`msend`, `%echo%`/`mecho`/`oecho`/`wecho`,
//! `%echoaround%`, `%damage%`, `%teleport%`, `%purge%`, `%load%`.
//!
//! ## Phase 2 scope (landed)
//!
//! - `wait <duration>` cooperative suspension via tokio task. Bodies that
//!   contain `wait` move to an async eval; the sync caller gets `Done`
//!   immediately and forfeits the ability to cancel host actions via
//!   `return 0`. Bodies without `wait` keep the synchronous fast path.
//! - `context %expr%` switches the durable scope for `global`/`unset`.
//! - `global <var>` promotes a local to either world `dg_globals`
//!   (no context) or the context entity's `dg_vars`.
//! - `unset <var>` clears local + durable scopes.
//! - `remote <var> <uuid>` / `rdelete <var> <uuid>` write or remove a
//!   var on a named entity's `dg_vars`.
//! - `%<uuid>.<field>%` reads `dg_vars[field]` from a remote entity.
//!
//! ## Phase 3 scope (landed)
//!
//! - New trigger types: `OnFight` / `OnHitPercent` (combat tick),
//!   `OnReceive` / `OnBribe` (give.rhai), `OnLoad` (spawn tick),
//!   `OnCommand` (room/item/mobile, args[0] keyword-matches the verb).
//! - `dg_cast '<spell>' <target>` — simplified mapping that treats the
//!   spell name as an [`crate::EffectType`] and applies a permanent-ish
//!   buff (default 5 min). Unknown effects silently no-op.
//! - `dg_affect <target> <effect> <magnitude> <duration>` — direct
//!   buff application (no spell-engine dependency).
//! - `mforce` / `oforce` / `wforce` / `force <target> <cmdline>` — inject
//!   a command line into a player target's input stream.
//! - Helpers [`fire_mobile_dg_triggers`] / [`fire_item_dg_triggers`] /
//!   [`fire_room_dg_triggers`] for Rust-side trigger fire sites that
//!   only need the DG path (no template/rhai dispatch).
//!
//! ## Phase 4 scope (landed)
//!
//! - `attach <vnum> <target>` / `detach <vnum> <target>` resolve a DG
//!   trigger prototype out of the `dg_trigger_protos` sled tree (seeded
//!   by the tbamud importer for every parsed `.trg` record) and push or
//!   pull a fully-bodied trigger on the named host. `target` accepts a
//!   UUID, a prototype vnum, or `self`/`here`.
//! - `dg_cast '<spell>' <target>` now handles three shapes: buffs/debuffs
//!   via `EffectType::from_str`, damage spells via [`cmds::dg_cast_damage_table`],
//!   and healing spells via [`cmds::dg_cast_heal_table`]. Unknown names
//!   silent-no-op.
//! - Vocabulary: `mremember` / `mforget` (mob memory), `mhunt`
//!   (pursuit_target), `mat <room> <cmd>` (rebind self_room and recurse),
//!   `mdoor` / `odoor` / `wdoor` (door state mutation: open/closed/locked/
//!   unlocked/pickproof/normal + description + purge).
//! - OnCommand wired into `src/lib.rs` command dispatch via
//!   [`fire_oncommand_for_player`]: fires room + room-mobs + player items
//!   triggers before the rhai run_command call, with `Return(0)` short-
//!   circuiting the host command.
//! - Builder UX: `medit/oedit/redit <id> trigger dg <list|view|add|edit|attach|protos>`
//!   — `add`/`edit` open the line editor in `collecting_dg_body` mode
//!   (mirrors `oedit note`), saving back into the trigger's `dg_body`.
//!   Helpers live in `scripts/lib/dg_olc.rhai`.
//!
//! ## Still deferred
//!
//! - Real spell-engine integration for `dg_cast` (currently table-driven
//!   for damage/heal; doesn't read `World.spell_definitions`).
//! - `mhunt` is a one-shot pursuit set; tbamud's `mhunt` includes
//!   automatic re-pathing across reboots.

pub mod analyze;
pub mod ast;
pub mod cmds;
pub mod eval;
pub mod mob_cmd;
pub mod parser;
pub mod vars;

use std::sync::Arc;
use uuid::Uuid;

use crate::SharedConnections;
use crate::db::Db;

/// What kind of entity a DG script is `%self%`-bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    Mob,
    Obj,
    Room,
}

/// One side of an `%actor%` / `%victim%` reference, resolved to either a
/// player (by connection_id + uuid + name) or a mobile (by uuid + name).
/// Field lookups (`%actor.level%`, `%actor.is_pc%`) read from this.
#[derive(Debug, Clone)]
pub enum ActorRef {
    Player { connection_id: String, char_id: Uuid, name: String },
    Mob { mobile_id: Uuid, name: String },
}

impl ActorRef {
    pub fn name(&self) -> &str {
        match self {
            ActorRef::Player { name, .. } | ActorRef::Mob { name, .. } => name.as_str(),
        }
    }

    pub fn is_pc(&self) -> bool {
        matches!(self, ActorRef::Player { .. })
    }
}

/// Runtime context for one DG script invocation. Holds the world handles,
/// the `%self%` binding, and any event-supplied actors / args.
///
/// Cloned cheaply — `Arc<Db>` and `SharedConnections` are reference-counted.
#[derive(Clone)]
pub struct EvalCtx {
    pub db: Arc<Db>,
    pub connections: SharedConnections,
    pub self_kind: SelfKind,
    pub self_id: Uuid,
    /// Cached `%self%` data fields (name, vnum, room) snapshotted at fire
    /// time. The interpreter never re-reads from `db` for these — keeping
    /// them inline avoids re-locking and matches DG's "self is fixed for
    /// this run" semantics.
    pub self_name: String,
    pub self_vnum: String,
    pub self_room: Option<Uuid>,
    pub actor: Option<ActorRef>,
    pub victim: Option<ActorRef>,
    /// `%arg%` — argument passed to a COMMAND/SPEECH trigger (the rest of
    /// the player's command line after the verb).
    pub arg: String,
    /// `%cmd%` — the verb the player typed (COMMAND triggers).
    pub cmd: String,
    /// `%cmd.mudcommand%` — canonical (un-abbreviated) command name.
    /// Equal to `cmd` when the player typed the verb in full; the
    /// canonical form when they used an abbreviation (e.g. `cmd="dr"` →
    /// `cmd_canonical="drop"`).
    pub cmd_canonical: String,
    /// Character name of the trigger's author (set on `dg_body` save).
    /// `None` for importer-seeded / system-authored triggers — these get
    /// the legacy "trusted" pass on dangerous opcodes.
    pub authored_by: Option<String>,
    /// Trigger-level elevation set by an admin via `trigger dg elevate`.
    /// When true, dangerous DG opcodes (force/at/purge/load/teleport)
    /// bypass the per-author area gate.
    pub elevated: bool,
    /// RAII cleanup for test databases.
    #[cfg(test)]
    pub test_temp_dir: Option<Arc<tempfile::TempDir>>,
}

impl EvalCtx {
    /// Permission gate consulted by dangerous DG commands (force, at,
    /// purge, load, teleport). The contract:
    ///
    /// - `elevated=true`         → allowed (admin-set override).
    /// - `authored_by=None`      → allowed (importer/system-authored).
    /// - admin-authored          → allowed regardless of area.
    /// - else                    → allowed only when the author can edit
    ///                             `target_area` (or the host's own area
    ///                             if `target_area` is None).
    ///
    /// Failure logs a warn and the caller no-ops the opcode.
    pub fn opcode_authorized(&self, opcode: &str, target_area: Option<Uuid>) -> bool {
        if self.elevated {
            return true;
        }
        let Some(author) = self.authored_by.as_deref() else {
            return true;
        };
        if let Ok(Some(ch)) = self.db.get_character_data(author) {
            if ch.is_admin {
                return true;
            }
        }
        let area_id = target_area.or_else(|| self.host_area_id());
        let Some(area_id) = area_id else {
            tracing::warn!(
                "[SECURITY] DG opcode '{}' blocked: trigger author '{}' tried to act on un-areaed target",
                opcode, author
            );
            return false;
        };
        let area = match self.db.get_area_data(&area_id) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        let allowed = crate::api::auth::author_can_edit_area(author, &area);
        if !allowed {
            tracing::warn!(
                "[SECURITY] DG opcode '{}' blocked: author '{}' lacks edit permission on area '{}'",
                opcode, author, area.name
            );
        }
        allowed
    }

    /// Best-effort lookup of the host entity's area id, used as the
    /// default target area when the opcode caller doesn't supply one.
    fn host_area_id(&self) -> Option<Uuid> {
        match self.self_kind {
            SelfKind::Mob => self
                .db
                .get_mobile_data(&self.self_id)
                .ok()
                .flatten()
                .and_then(|m| m.area_id),
            SelfKind::Obj => self
                .db
                .get_item_data(&self.self_id)
                .ok()
                .flatten()
                .and_then(|i| i.area_id),
            SelfKind::Room => self
                .db
                .get_room_data(&self.self_id)
                .ok()
                .flatten()
                .and_then(|r| r.area_id),
        }
    }
}

/// Push a writer-mistake notice to the BUILDER DEBUG channel. The message
/// is prefixed with the trigger's host (`[DG mob guard]`) and suffixed with
/// the author when known. Consecutive duplicates are suppressed so a
/// periodic trigger doesn't saturate the 50-slot ring buffer.
///
/// Reserved for author errors — unknown verbs, malformed args, lookup
/// failures that imply a typo. Gameplay-state no-ops (target left the
/// room, item not in inventory) must stay silent.
pub(crate) fn warn_builder(ctx: &EvalCtx, msg: &str) {
    let kind = match ctx.self_kind {
        SelfKind::Mob => "mob",
        SelfKind::Obj => "obj",
        SelfKind::Room => "room",
    };
    let author = match ctx.authored_by.as_deref() {
        Some(a) if !a.is_empty() => format!(" (by {a})"),
        _ => String::new(),
    };
    let line = format!("[DG {kind} {}] {msg}{author}", ctx.self_name);
    crate::session::broadcast::broadcast_to_builders_dedup(&ctx.connections, &line);
}

/// Result of evaluating a DG body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Script finished normally.
    Done,
    /// Script hit a `halt` statement.
    Halt,
    /// Script hit a `return <n>` statement. `0` is conventionally used by
    /// COMMAND triggers to suppress the host action; non-zero lets it
    /// proceed.
    Return(i32),
    /// Phase-2 placeholder. The evaluator currently treats `wait` as a
    /// no-op; this variant exists so call sites can be future-proofed.
    Suspended,
}

/// Public entry point: parse `body` and evaluate against `ctx`.
///
/// Errors during parsing or evaluation are folded into [`Outcome::Done`]
/// after logging — DG triggers must never crash the host even on bad input.
///
/// ## Sync vs. async dispatch
///
/// - When the body has no `wait`, the eval runs synchronously and the
///   real outcome (including `Return(0)` cancellation) is observable to
///   the caller. This is the common case (~90% of stock triggers).
/// - When the body contains `wait` (recursively, at any depth), eval is
///   moved to a tokio task so [`tokio::time::sleep`] can suspend at the
///   wait point. The synchronous return value is always [`Outcome::Done`]
///   — the script's eventual return is no longer observable. This
///   mirrors tbamud's behavior: a wait-bearing script gives up its veto.
pub fn fire_dg(body: &str, ctx: &EvalCtx) -> Outcome {
    let block = match parser::parse(body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("DG parse error in trigger on {} ({:?}): {}", ctx.self_name, ctx.self_kind, e);
            warn_builder(ctx, &format!("parse error: {e}"));
            return Outcome::Done;
        }
    };

    // Sync fast path: no wait → real outcome.
    if !eval::block_contains_wait(&block) {
        let mut state = eval::State::new();
        return match eval::eval_block(&block, ctx, &mut state) {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(
                    "DG eval error in trigger on {} ({:?}): {}",
                    ctx.self_name, ctx.self_kind, e
                );
                warn_builder(ctx, &format!("eval error: {e}"));
                Outcome::Done
            }
        };
    }

    // Async path: spawn into the surrounding tokio runtime. If we're
    // not in a runtime (only happens in tests/CLI tools), fall back to
    // running the sync path with `wait` treated as a no-op so we don't
    // deadlock or panic.
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            let ctx_owned = ctx.clone();
            let block_owned = block;
            let self_name = ctx.self_name.clone();
            let self_kind = ctx.self_kind;
            handle.spawn(async move {
                let mut state = eval::State::new();
                if let Err(e) = eval::eval_block_async(&block_owned, &ctx_owned, &mut state).await {
                    tracing::warn!(
                        "DG eval error in async trigger on {} ({:?}): {}",
                        self_name, self_kind, e
                    );
                    warn_builder(&ctx_owned, &format!("eval error: {e}"));
                }
            });
            Outcome::Done
        }
        Err(_) => {
            let mut state = eval::State::new();
            eval::eval_block(&block, ctx, &mut state).unwrap_or(Outcome::Done)
        }
    }
}

/// Bridge helper for the Rhai-side `fire_room_trigger`: builds an
/// [`EvalCtx`] from the entity + event data and runs the body.
///
/// `authored_by` / `elevated` are propagated from the caller's
/// `RoomTrigger` so the DG opcode gate can scope dangerous verbs.
/// Pass `(None, false)` for legacy/test sites that don't have a
/// trigger struct on hand — those run with the "system-authored"
/// trusted pass.
pub fn fire_room_dg(
    body: &str,
    room: &crate::types::RoomData,
    connection_id: &str,
    db: Arc<Db>,
    connections: SharedConnections,
    authored_by: Option<String>,
    elevated: bool,
) -> Outcome {
    let actor = actor_from_connection(connection_id, &connections);
    let ctx = EvalCtx {
        db,
        connections,
        self_kind: SelfKind::Room,
        self_id: room.id,
        self_name: room.title.clone(),
        self_vnum: room.vnum.clone().unwrap_or_default(),
        self_room: Some(room.id),
        actor,
        victim: None,
        arg: String::new(),
        cmd: String::new(),
        cmd_canonical: String::new(),
        authored_by,
        elevated,
        #[cfg(test)]
        test_temp_dir: None,
    };
    fire_dg(body, &ctx)
}

/// Bridge helper for `fire_item_trigger`. See [`fire_room_dg`] for
/// `authored_by` / `elevated` semantics.
pub fn fire_item_dg(
    body: &str,
    item: &crate::types::ItemData,
    connection_id: &str,
    db: Arc<Db>,
    connections: SharedConnections,
    authored_by: Option<String>,
    elevated: bool,
) -> Outcome {
    let actor = actor_from_connection(connection_id, &connections);
    let self_room = match &item.location {
        crate::types::ItemLocation::Room(r) => Some(*r),
        _ => actor_room(actor.as_ref(), &db),
    };
    let ctx = EvalCtx {
        db,
        connections,
        self_kind: SelfKind::Obj,
        self_id: item.id,
        self_name: item.name.clone(),
        self_vnum: item.vnum.clone().unwrap_or_default(),
        self_room,
        actor,
        victim: None,
        arg: String::new(),
        cmd: String::new(),
        cmd_canonical: String::new(),
        authored_by,
        elevated,
        #[cfg(test)]
        test_temp_dir: None,
    };
    fire_dg(body, &ctx)
}

/// Bridge helper for `fire_mobile_trigger`. See [`fire_room_dg`] for
/// `authored_by` / `elevated` semantics.
pub fn fire_mobile_dg(
    body: &str,
    mobile: &crate::types::MobileData,
    connection_id: &str,
    db: Arc<Db>,
    connections: SharedConnections,
    authored_by: Option<String>,
    elevated: bool,
) -> Outcome {
    let actor = actor_from_connection(connection_id, &connections);
    let ctx = EvalCtx {
        db,
        connections,
        self_kind: SelfKind::Mob,
        self_id: mobile.id,
        self_name: mobile.name.clone(),
        self_vnum: mobile.vnum.clone(),
        self_room: mobile.current_room_id,
        actor,
        victim: None,
        arg: String::new(),
        cmd: String::new(),
        cmd_canonical: String::new(),
        authored_by,
        elevated,
        #[cfg(test)]
        test_temp_dir: None,
    };
    fire_dg(body, &ctx)
}

/// Fire any DG-bodied triggers of `trig_type` on the given item. Returns
/// `true` if any trigger returned 0 (cancel host action), `false` otherwise.
/// Phase-3 helper — used for `OnLoad` / `OnCommand` on items where we
/// don't need the full template/rhai dispatch path that mobiles use.
pub fn fire_item_dg_triggers(
    db: &Arc<Db>,
    connections: &SharedConnections,
    item: &crate::types::ItemData,
    trig_type: crate::types::ItemTriggerType,
    connection_id: &str,
) -> bool {
    let mut cancelled = false;
    for t in &item.triggers {
        if t.trigger_type != trig_type || !t.enabled {
            continue;
        }
        let Some(body) = t.dg_body.as_deref() else {
            continue;
        };
        let outcome = fire_item_dg(
            body,
            item,
            connection_id,
            db.clone(),
            connections.clone(),
            t.authored_by.clone(),
            t.elevated,
        );
        if matches!(outcome, Outcome::Return(0)) {
            cancelled = true;
        }
    }
    cancelled
}

/// Fire any DG-bodied triggers of `trig_type` on the given room.
pub fn fire_room_dg_triggers(
    db: &Arc<Db>,
    connections: &SharedConnections,
    room: &crate::types::RoomData,
    trig_type: crate::types::TriggerType,
    connection_id: &str,
    cmd: &str,
    cmd_canonical: &str,
    arg: &str,
) -> bool {
    let mut cancelled = false;
    for t in &room.triggers {
        if t.trigger_type != trig_type || !t.enabled {
            continue;
        }
        let Some(body) = t.dg_body.as_deref() else {
            continue;
        };
        // For OnCommand, the trigger's args[0] is the verb keyword.
        // Skip the trigger if its verb doesn't match (DG `/=` semantics:
        // case-insensitive equality OR mutual prefix match).
        if trig_type == crate::types::TriggerType::OnCommand {
            if let Some(want) = t.args.first() {
                if !dg_keyword_match(want, cmd) {
                    continue;
                }
            }
        }
        let actor = actor_from_connection(connection_id, connections);
        let ctx = EvalCtx {
            db: db.clone(),
            connections: connections.clone(),
            self_kind: SelfKind::Room,
            self_id: room.id,
            self_name: room.title.clone(),
            self_vnum: room.vnum.clone().unwrap_or_default(),
            self_room: Some(room.id),
            actor,
            victim: None,
            arg: arg.to_string(),
            cmd: cmd.to_string(),
            cmd_canonical: if cmd_canonical.is_empty() {
                cmd.to_string()
            } else {
                cmd_canonical.to_string()
            },
            authored_by: t.authored_by.clone(),
            elevated: t.elevated,
            #[cfg(test)]
            test_temp_dir: None,
        };
        if matches!(fire_dg(body, &ctx), Outcome::Return(0)) {
            cancelled = true;
        }
    }
    cancelled
}

/// Fire any DG-bodied triggers of `trig_type` on the given mobile, with
/// optional `cmd`/`arg` for `OnCommand`-shape triggers and an optional
/// extra `actor` (e.g. the giver in `OnReceive`).
pub fn fire_mobile_dg_triggers(
    db: &Arc<Db>,
    connections: &SharedConnections,
    mobile: &crate::types::MobileData,
    trig_type: crate::types::MobileTriggerType,
    connection_id: &str,
    cmd: &str,
    cmd_canonical: &str,
    arg: &str,
) -> bool {
    let mut cancelled = false;
    for t in &mobile.triggers {
        if t.trigger_type != trig_type || !t.enabled {
            continue;
        }
        let Some(body) = t.dg_body.as_deref() else {
            continue;
        };
        if trig_type == crate::types::MobileTriggerType::OnCommand {
            if let Some(want) = t.args.first() {
                if !dg_keyword_match(want, cmd) {
                    continue;
                }
            }
        }
        let actor = actor_from_connection(connection_id, connections);
        let ctx = EvalCtx {
            db: db.clone(),
            connections: connections.clone(),
            self_kind: SelfKind::Mob,
            self_id: mobile.id,
            self_name: mobile.name.clone(),
            self_vnum: mobile.vnum.clone(),
            self_room: mobile.current_room_id,
            actor,
            victim: None,
            arg: arg.to_string(),
            cmd: cmd.to_string(),
            cmd_canonical: if cmd_canonical.is_empty() {
                cmd.to_string()
            } else {
                cmd_canonical.to_string()
            },
            authored_by: t.authored_by.clone(),
            elevated: t.elevated,
            #[cfg(test)]
            test_temp_dir: None,
        };
        if matches!(fire_dg(body, &ctx), Outcome::Return(0)) {
            cancelled = true;
        }
    }
    cancelled
}

/// Fire OnCommand DG triggers for a player command across the player's
/// room, the mobs in that room, and any items the player has in inventory
/// or equipped. Returns `true` if any trigger returned 0, signalling the
/// host command should be cancelled. Phase-4 hook used by lib.rs's command
/// dispatch loop just before the rhai run_command call.
pub fn fire_oncommand_for_player(
    connection_id: &Uuid,
    cmd: &str,
    cmd_canonical: &str,
    arg: &str,
    db: &Arc<Db>,
    connections: &SharedConnections,
) -> bool {
    // Snapshot what we need from the session under one lock.
    let (char_name, room_id) = {
        let Ok(conns) = connections.lock() else { return false };
        let Some(session) = conns.get(connection_id) else { return false };
        let Some(ch) = session.character.as_ref() else { return false };
        (ch.name.clone(), ch.current_room_id)
    };

    let conn_id_str = connection_id.to_string();
    let mut cancelled = false;

    // Room-level OnCommand triggers.
    if let Ok(Some(room)) = db.get_room_data(&room_id) {
        if fire_room_dg_triggers(
            db,
            connections,
            &room,
            crate::types::TriggerType::OnCommand,
            &conn_id_str,
            cmd,
            cmd_canonical,
            arg,
        ) {
            cancelled = true;
        }
    }

    // Mobs in the room.
    if let Ok(mobs) = db.get_mobiles_in_room(&room_id) {
        for mob in mobs {
            if fire_mobile_dg_triggers(
                db,
                connections,
                &mob,
                crate::types::MobileTriggerType::OnCommand,
                &conn_id_str,
                cmd,
                cmd_canonical,
                arg,
            ) {
                cancelled = true;
            }
        }
    }

    // Items in inventory + equipped.
    let mut items: Vec<crate::types::ItemData> = Vec::new();
    if let Ok(inv) = db.get_items_in_inventory(&char_name) {
        items.extend(inv);
    }
    if let Ok(eq) = db.get_equipped_items(&char_name) {
        items.extend(eq);
    }
    for item in items {
        // Items don't have OnCommand args in Phase 3 yet — fire helper does
        // its own keyword gating via the trigger's args[0].
        if fire_item_dg_oncommand(db, connections, &item, &conn_id_str, cmd, cmd_canonical, arg) {
            cancelled = true;
        }
    }

    cancelled
}

/// Item OnCommand fire — same shape as `fire_item_dg_triggers` but with
/// cmd/arg substitution and keyword gating. Inlined here because the
/// Phase-3 helper signature doesn't take cmd/arg.
fn fire_item_dg_oncommand(
    db: &Arc<Db>,
    connections: &SharedConnections,
    item: &crate::types::ItemData,
    connection_id: &str,
    cmd: &str,
    cmd_canonical: &str,
    arg: &str,
) -> bool {
    let mut cancelled = false;
    for t in &item.triggers {
        if t.trigger_type != crate::types::ItemTriggerType::OnCommand || !t.enabled {
            continue;
        }
        let Some(body) = t.dg_body.as_deref() else {
            continue;
        };
        if let Some(want) = t.args.first() {
            if !dg_keyword_match(want, cmd) {
                continue;
            }
        }
        let actor = actor_from_connection(connection_id, connections);
        let self_room = match &item.location {
            crate::types::ItemLocation::Room(r) => Some(*r),
            _ => actor_room(actor.as_ref(), db),
        };
        let ctx = EvalCtx {
            db: db.clone(),
            connections: connections.clone(),
            self_kind: SelfKind::Obj,
            self_id: item.id,
            self_name: item.name.clone(),
            self_vnum: item.vnum.clone().unwrap_or_default(),
            self_room,
            actor,
            victim: None,
            arg: arg.to_string(),
            cmd: cmd.to_string(),
            cmd_canonical: if cmd_canonical.is_empty() {
                cmd.to_string()
            } else {
                cmd_canonical.to_string()
            },
            authored_by: t.authored_by.clone(),
            elevated: t.elevated,
            #[cfg(test)]
            test_temp_dir: None,
        };
        if matches!(fire_dg(body, &ctx), Outcome::Return(0)) {
            cancelled = true;
        }
    }
    cancelled
}

/// DG `/=` keyword match: case-insensitive equality OR mutual prefix.
/// Stock command-trigger args use this to match verb prefixes
/// (e.g. trigger arg `pan` matches player typing `pa` or `pann`).
fn dg_keyword_match(want: &str, got: &str) -> bool {
    let w = want.trim().to_ascii_lowercase();
    let g = got.trim().to_ascii_lowercase();
    if w.is_empty() {
        return true;
    }
    if g.is_empty() {
        return false;
    }
    w == g || w.starts_with(&g) || g.starts_with(&w)
}

/// Resolve a connection_id string to a fully-bound [`ActorRef::Player`]
/// by looking up the active session. Returns None when the id is empty,
/// unparseable, or stale (no matching session).
fn actor_from_connection(connection_id: &str, connections: &SharedConnections) -> Option<ActorRef> {
    if connection_id.is_empty() {
        return None;
    }
    let cid = Uuid::parse_str(connection_id).ok()?;
    let conns = connections.lock().ok()?;
    let session = conns.get(&cid)?;
    let ch = session.character.as_ref()?;
    Some(ActorRef::Player {
        connection_id: connection_id.to_string(),
        char_id: cid, // ConnectionId is the most stable identifier we have
        name: ch.name.clone(),
    })
}

/// For item triggers fired without a room context (e.g. an item in a
/// player's inventory), fall back to the actor's current room so
/// `%self.room%` and `wecho`-style commands have a sane target.
fn actor_room(actor: Option<&ActorRef>, db: &Db) -> Option<Uuid> {
    let name = match actor? {
        ActorRef::Player { name, .. } if !name.is_empty() => name.clone(),
        _ => return None,
    };
    db.get_character_data(&name).ok().flatten().map(|c| c.current_room_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Build a minimal EvalCtx against a temp db, with no live player
    /// session. Sufficient to exercise control flow, vars, and the
    /// no-actor branches of cmds.
    fn make_ctx(self_kind: SelfKind, self_id: Uuid, self_name: &str) -> EvalCtx {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().to_owned();
        let db = Arc::new(Db::open(&path).expect("open db"));
        let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        EvalCtx {
            db,
            connections,
            self_kind,
            self_id,
            self_name: self_name.to_string(),
            self_vnum: "test".to_string(),
            self_room: None,
            actor: None,
            victim: None,
            arg: String::new(),
            cmd: String::new(),
            cmd_canonical: String::new(),
            authored_by: None,
            elevated: false,
            test_temp_dir: Some(Arc::new(temp)),
        }
    }

    #[test]
    fn fire_dg_returns_zero_from_command_trigger() {
        // Mirrors `if %direction% == south\n  return 0\nend` from the
        // mage guildguard fixture. Without `direction` bound, no branch
        // matches and the script runs to completion → Done.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "guard");
        let body = "\
if %direction% == south
  return 0
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Done);

        // With the local set, the if matches and we return 0.
        let body2 = "\
set direction south
if %direction% == south
  return 0
end";
        assert_eq!(fire_dg(body2, &ctx), Outcome::Return(0));
    }

    #[test]
    fn fire_dg_executes_switch_with_fallthrough() {
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "wizard");
        ctx.cmd = "test".to_string();
        // Match arm 'a' falls through to 'b' which returns 1.
        let body = "\
switch a
  case a
  case b
    return 7
  case c
    return 9
done";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(7));
    }

    #[test]
    fn fire_dg_halts_on_halt_statement() {
        let ctx = make_ctx(SelfKind::Room, Uuid::new_v4(), "river");
        let body = "halt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
    }

    #[test]
    fn fire_dg_set_and_eval_arithmetic() {
        // `set i 5` then `eval i %i% + 1` should leave i=6.
        // We can't observe locals from outside, so this test verifies the
        // script reaches Done without crashing on the eval expression.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "merchant");
        let body = "\
set i 5
eval i %i% + 1
if %i% == 6
  halt
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
    }

    #[test]
    fn fire_dg_unknown_command_silently_continues() {
        // dg_cast / mforce etc. are Phase-3; they should silent-no-op so
        // imported scripts with them don't blow up.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "casting_mob");
        let body = "\
dg_cast 'fireball' victim
mforce victim flee
halt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
    }

    // ---------- Phase 2 tests ----------

    #[test]
    fn global_promotes_local_to_world_store() {
        // After `global foo`, the value flows into Db.dg_globals and
        // survives across separate `fire_dg` calls.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "merchant");
        let body1 = "\
set greeting hello
global greeting
halt";
        assert_eq!(fire_dg(body1, &ctx), Outcome::Halt);
        assert_eq!(
            ctx.db.get_dg_global("greeting").expect("get").as_deref(),
            Some("hello")
        );

        // A second script with no local should read through to the global.
        // We can't observe interpreter state directly, but we can verify
        // the value via the db.
        let body2 = "\
global greeting
halt";
        assert_eq!(fire_dg(body2, &ctx), Outcome::Halt);
        // Greeting still set (no local to promote, but no clear either).
        assert_eq!(
            ctx.db.get_dg_global("greeting").expect("get").as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn unset_clears_local_and_global() {
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "merchant");
        let body = "\
set foo bar
global foo
unset foo
halt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
        assert_eq!(ctx.db.get_dg_global("foo").expect("get"), None);
    }

    #[test]
    fn context_redirects_durable_writes_to_entity_vars() {
        // `context %self.id%` makes `global` write to the mob's dg_vars
        // instead of world globals.
        let mob = crate::types::MobileData::new("test_mob".to_string());
        let mob_id = mob.id;
        let ctx = make_ctx(SelfKind::Mob, mob_id, "test_mob");
        ctx.db.save_mobile_data(mob).expect("save");

        let body = "\
set tag friendly
context %self.id%
global tag
halt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);

        // World globals untouched.
        assert_eq!(ctx.db.get_dg_global("tag").expect("get"), None);
        // Mob's dg_vars carries the value.
        let saved = ctx.db.get_mobile_data(&mob_id).expect("get").expect("mob");
        assert_eq!(saved.dg_vars.get("tag").map(|s| s.as_str()), Some("friendly"));
    }

    #[test]
    fn remote_writes_var_to_named_entity() {
        let target = crate::types::MobileData::new("target_mob".to_string());
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "writer_mob");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!(
            "\
set color red
remote color {tid}
halt",
            tid = target_id
        );
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert_eq!(saved.dg_vars.get("color").map(|s| s.as_str()), Some("red"));
    }

    #[test]
    fn rdelete_removes_remote_var() {
        let mut target = crate::types::MobileData::new("target_mob".to_string());
        target.dg_vars.insert("color".to_string(), "blue".to_string());
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "writer_mob");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("rdelete color {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert!(!saved.dg_vars.contains_key("color"));
    }

    #[test]
    fn remote_uuid_var_lookup_in_substitution() {
        // `%<uuid>.<field>%` reads dg_vars[field] on the entity.
        let mut target = crate::types::MobileData::new("target_mob".to_string());
        target.dg_vars.insert("mood".to_string(), "happy".to_string());
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "reader_mob");
        ctx.db.save_mobile_data(target).expect("save");

        // If the read works, set local copy will match → take the if branch
        // → halt. If the read fails (empty), the if won't match → done.
        let body = format!(
            "\
set m %{tid}.mood%
if %m% == happy
  halt
end",
            tid = target_id
        );
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wait_suspends_async_eval() {
        // Body containing `wait` must route through the async path; the
        // outer fire_dg returns Done immediately while the post-wait
        // stmts run after the sleep completes.
        let mob = crate::types::MobileData::new("waiter".to_string());
        let mob_id = mob.id;
        let ctx = make_ctx(SelfKind::Mob, mob_id, "waiter");
        ctx.db.save_mobile_data(mob).expect("save");

        let body = "\
context %self.id%
set ready no
global ready
wait 1 sec
set ready yes
global ready
halt";
        let outcome = fire_dg(body, &ctx);
        assert_eq!(outcome, Outcome::Done);

        // Wait for the spawned task to finish (sleep + post-wait writes).
        // Add a small buffer past the 1-second wait.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let post = ctx.db.get_mobile_data(&mob_id).expect("get").expect("mob");
        assert_eq!(
            post.dg_vars.get("ready").map(|s| s.as_str()),
            Some("yes"),
            "post-wait write must land in mob.dg_vars"
        );
    }

    #[test]
    fn fire_dg_real_panning_trigger_parses_and_evaluates() {
        // Full body from tbamud 7.trg #700. We don't have a real actor or
        // held object so the script just runs the outer guards to no-op.
        let ctx = make_ctx(SelfKind::Room, Uuid::new_v4(), "river_bank");
        let body = "\
if %actor.move% <= 10
  %send% %actor% You are too exhausted.
  halt
end
if %cmd% /= pan && %arg% /= gold
  eval heldobj %actor.eq(hold)%
  if %heldobj.vnum% == 717
    %send% %actor% You dip your pan into the river...
    nop %actor.move(-10)%
    wait 3 sec
    if %random.10% == 1
      %load% obj 718 %actor% inv
    end
  end
end";
        // No actor → %actor.move% is empty → first if `<= 10` evaluates
        // empty-vs-10. Empty parses as not-an-int; comparison falls to
        // string. We just want this to not crash.
        let _ = fire_dg(body, &ctx);
    }

    // ---------- Phase 3 tests ----------

    #[test]
    fn dg_affect_applies_buff_to_targeted_mob() {
        // `dg_affect <uuid> sleep 1 30` should land a Sleep buff on the
        // target mob with magnitude=1, duration=30.
        let target = crate::types::MobileData::new("target".to_string());
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "caster");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("dg_affect {tid} sleep 1 30\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        let buff = saved
            .active_buffs
            .iter()
            .find(|b| b.effect_type == crate::EffectType::Sleep);
        assert!(buff.is_some(), "Sleep buff must be applied via dg_affect");
        let buff = buff.unwrap();
        assert_eq!(buff.magnitude, 1);
        assert_eq!(buff.remaining_secs, 30);
    }

    #[test]
    fn dg_cast_with_known_spell_applies_effect() {
        // `dg_cast 'sleep' <uuid>` (quoted spell name) → simplified mapping
        // treats spell name as effect → applies Sleep buff.
        let target = crate::types::MobileData::new("victim".to_string());
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "caster");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("dg_cast 'sleep' {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert!(
            saved
                .active_buffs
                .iter()
                .any(|b| b.effect_type == crate::EffectType::Sleep),
            "dg_cast 'sleep' must produce a Sleep buff"
        );
    }

    #[test]
    fn dg_cast_with_unknown_effect_is_silent_noop() {
        // `dg_cast 'nonsense' victim` doesn't crash; just no-ops.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "caster");
        let body = "dg_cast 'nonsense' victim\nhalt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
    }

    #[test]
    fn keyword_match_handles_prefixes_and_case() {
        assert!(dg_keyword_match("pan", "pan"));
        assert!(dg_keyword_match("pan", "PAN"));
        assert!(dg_keyword_match("pan", "pa")); // mutual prefix
        assert!(dg_keyword_match("pan", "panning")); // prefix the other way
        assert!(!dg_keyword_match("pan", "shovel"));
        // Empty `want` (no first arg) is the always-match case.
        assert!(dg_keyword_match("", "anything"));
    }

    // ---------- Phase 4 tests ----------

    #[test]
    fn dg_cast_fireball_subtracts_hp() {
        let mut target = crate::types::MobileData::new("dummy".to_string());
        target.max_hp = 100;
        target.current_hp = 100;
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "caster");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("dg_cast 'fireball' {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert_eq!(saved.current_hp, 70, "fireball should subtract 30 HP");
    }

    #[test]
    fn dg_cast_heal_adds_hp_capped_at_max() {
        let mut target = crate::types::MobileData::new("patient".to_string());
        target.max_hp = 100;
        target.current_hp = 50;
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "healer");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("dg_cast 'heal' {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert_eq!(saved.current_hp, 100, "heal caps at max_hp");
    }

    #[test]
    fn dg_cast_cure_blind_removes_blind_buff() {
        // Plant a Blind buff on a target mob, then have a script run
        // `dg_cast 'cure_blind' <target>` and assert the buff is stripped.
        let mut target = crate::types::MobileData::new("subject".to_string());
        target.active_buffs.push(crate::ActiveBuff {
            effect_type: crate::EffectType::Blind,
            magnitude: 1,
            remaining_secs: 60,
            source: "old_caster".to_string(),
            damage_type: None,
            vs_effect: None,
        });
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "healer");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("dg_cast 'cure_blind' {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert!(
            !saved
                .active_buffs
                .iter()
                .any(|b| b.effect_type == crate::EffectType::Blind),
            "cure_blind must remove the Blind buff"
        );
    }

    #[test]
    fn dg_cast_armor_alias_lands_ac_boost() {
        // The `armor` spell name is a tbamud-stock alias for ArmorClassBoost.
        let target = crate::types::MobileData::new("subject".to_string());
        let target_id = target.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "caster");
        ctx.db.save_mobile_data(target).expect("save");

        let body = format!("dg_cast 'armor' {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&target_id).expect("get").expect("mob");
        assert!(
            saved
                .active_buffs
                .iter()
                .any(|b| b.effect_type == crate::EffectType::ArmorClassBoost),
            "armor must alias to ArmorClassBoost"
        );
    }

    #[test]
    fn dg_attach_clones_prototype_onto_target() {
        // Seed a prototype, then run `attach <vnum> <target_uuid>` on a mob.
        let host = crate::types::MobileData::new("host_mob".to_string());
        let host_id = host.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "self_mob");
        ctx.db.save_mobile_data(host).expect("save");

        let proto = crate::types::DgTriggerProto {
            vnum: "9999".to_string(),
            name: "test greet".to_string(),
            attach_kind: crate::types::DgAttachKind::Mob,
            flags: "g".to_string(),
            numeric_arg: 100,
            arglist: String::new(),
            body: "halt".to_string(),
        };
        ctx.db.save_dg_trigger_proto(&proto).expect("save proto");

        let body = format!("attach 9999 {hid}\nhalt", hid = host_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&host_id).expect("get").expect("host");
        assert_eq!(saved.triggers.len(), 1, "attach should add one trigger");
        assert_eq!(saved.triggers[0].dg_name.as_deref(), Some("test greet"));
        assert!(saved.triggers[0].dg_body.is_some());
    }

    #[test]
    fn dg_detach_removes_attached_trigger() {
        let mut host = crate::types::MobileData::new("host_mob".to_string());
        host.triggers.push(crate::MobileTrigger {
            trigger_type: crate::MobileTriggerType::OnGreet,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: Vec::new(),
            interval_secs: 60,
            last_fired: 0,
            dg_body: Some("halt".to_string()),
            dg_name: Some("removable".to_string()),
            authored_by: None,
            elevated: false,
            source_proto_vnum: None,
        });
        let host_id = host.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "self_mob");
        ctx.db.save_mobile_data(host).expect("save");

        let proto = crate::types::DgTriggerProto {
            vnum: "8888".to_string(),
            name: "removable".to_string(),
            attach_kind: crate::types::DgAttachKind::Mob,
            flags: "g".to_string(),
            numeric_arg: 100,
            arglist: String::new(),
            body: "halt".to_string(),
        };
        ctx.db.save_dg_trigger_proto(&proto).expect("save proto");

        let body = format!("detach 8888 {hid}\nhalt", hid = host_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&host_id).expect("get").expect("host");
        assert_eq!(saved.triggers.len(), 0, "detach should remove the matching trigger");
    }

    #[test]
    fn cmd_mudcommand_resolves_to_canonical_form() {
        // Player typed `dr`; OnCommand fire site populates cmd_canonical
        // with the resolved form `drop`. `%cmd%` returns `dr`,
        // `%cmd.mudcommand%` returns `drop`.
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "guard");
        ctx.cmd = "dr".to_string();
        ctx.cmd_canonical = "drop".to_string();

        let body = "\
if %cmd.mudcommand% == drop
  return 0
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(0));

        // When canonical is empty, %cmd.mudcommand% falls back to %cmd%.
        let mut ctx2 = make_ctx(SelfKind::Mob, Uuid::new_v4(), "guard");
        ctx2.cmd = "drop".to_string();
        ctx2.cmd_canonical = String::new();
        let body2 = "\
if %cmd.mudcommand% == drop
  return 0
end";
        assert_eq!(fire_dg(body2, &ctx2), Outcome::Return(0));
    }

    #[test]
    fn actor_id_resolves_for_player_and_mob() {
        // Player actor: %actor.id% == name (PCs are keyed by name in
        // IronMUD; this lets `remote ... %actor.id%` route through
        // `parse_scope_ref` to the character's `dg_vars`).
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "tester".to_string(),
        });
        let body = "\
set aid %actor.id%
if %aid% == tester
  return 7
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(7));

        // Mob actor: %actor.id% == mobile_id.
        let mob_id = Uuid::new_v4();
        let mut ctx2 = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        ctx2.actor = Some(ActorRef::Mob {
            mobile_id: mob_id,
            name: "minion".to_string(),
        });
        let body2 = format!(
            "\
set aid %actor.id%
if %aid% == {mid}
  return 8
end",
            mid = mob_id
        );
        assert_eq!(fire_dg(&body2, &ctx2), Outcome::Return(8));
    }

    #[test]
    fn remote_writes_to_player_dg_vars() {
        // tbamud's `remote VAR %actor.id% VALUE` pattern: write a var on
        // the player's persistent `dg_vars` so a later trigger can read it
        // via `%actor.VAR%`.
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "\
set cookie_day 42
remote cookie_day %actor.id%
if %actor.cookie_day% == 42
  return 11
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(11));

        // Persisted on disk.
        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert_eq!(after.dg_vars.get("cookie_day").map(String::as_str), Some("42"));
    }

    #[test]
    fn rdelete_clears_player_dg_var() {
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let mut ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.dg_vars.insert("score".to_string(), "99".to_string());
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "rdelete score %actor.id%";
        let _ = fire_dg(body, &ctx);
        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert!(after.dg_vars.get("score").is_none());
    }

    #[test]
    fn context_durable_writes_target_player() {
        // `context %actor.id%` + `global var` + `set var V` should write
        // V into the player's dg_vars.
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "\
context %actor.id%
global quest_step
set quest_step 3";
        let _ = fire_dg(body, &ctx);
        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert_eq!(after.dg_vars.get("quest_step").map(String::as_str), Some("3"));
    }

    #[test]
    fn actor_pronoun_fields_resolve_for_player() {
        // Save a male character and confirm heshe/himher/hisher resolve.
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
            "gender": "male",
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "\
if %actor.heshe% == he
  if %actor.himher% == him
    if %actor.hisher% == his
      return 9
    end
  end
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(9));
    }

    // ---------- Phase 5d tests ----------

    #[test]
    fn actor_gold_call_subtracts_and_persists() {
        // Player has 100 gold. Trigger does `nop %actor.gold(-50)%`. After
        // the fire, the character's persisted gold is 50.
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let mut ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "buyer",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.gold = 100;
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "buyer".to_string(),
        });

        let body = "nop %actor.gold(-50)%\nhalt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
        let after = ctx.db.get_character_data("buyer").unwrap().unwrap();
        assert_eq!(after.gold, 50, "gold(-50) should subtract 50");
    }

    #[test]
    fn actor_gold_call_clamps_at_zero() {
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let mut ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "broke",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.gold = 5;
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "broke".to_string(),
        });

        let body = "nop %actor.gold(-100)%\nhalt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
        let after = ctx.db.get_character_data("broke").unwrap().unwrap();
        assert_eq!(after.gold, 0, "gold call clamps at 0");
    }

    #[test]
    fn self_hp_call_heals_capped_at_max() {
        let mut mob = crate::types::MobileData::new("hurt".to_string());
        mob.is_prototype = false;
        mob.max_hp = 50;
        mob.current_hp = 30;
        let mob_id = mob.id;
        let ctx = make_ctx(SelfKind::Mob, mob_id, "hurt");
        ctx.db.save_mobile_data(mob).expect("save");

        let body = "nop %self.hitp(40)%\nhalt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);
        let after = ctx.db.get_mobile_data(&mob_id).unwrap().unwrap();
        assert_eq!(after.current_hp, 50, "self.hitp(N) caps at max_hp");
    }

    #[test]
    fn fire_mobile_dg_triggers_filters_by_command_keyword() {
        // OnCommand triggers gate on args[0] /= cmd. Build a mob with two
        // OnCommand triggers: one wants `pan`, one wants `shovel`. Fire
        // for `cmd=pan`; only the `pan` trigger should land its effect.
        let mut mob = crate::types::MobileData::new("guardian".to_string());
        mob.triggers.push(crate::MobileTrigger {
            trigger_type: crate::MobileTriggerType::OnCommand,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: vec!["pan".to_string()],
            interval_secs: 60,
            last_fired: 0,
            dg_body: Some("set fired pan\nglobal fired\nhalt".to_string()),
            dg_name: None,
            authored_by: None,
            elevated: false,
            source_proto_vnum: None,
        });
        mob.triggers.push(crate::MobileTrigger {
            trigger_type: crate::MobileTriggerType::OnCommand,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: vec!["shovel".to_string()],
            interval_secs: 60,
            last_fired: 0,
            dg_body: Some("set fired shovel\nglobal fired\nhalt".to_string()),
            dg_name: None,
            authored_by: None,
            elevated: false,
            source_proto_vnum: None,
        });
        let mob_id = mob.id;
        let ctx = make_ctx(SelfKind::Mob, mob_id, "guardian");
        ctx.db.save_mobile_data(mob.clone()).expect("save");

        // Reload to get the same shape and fire for cmd=pan.
        let mob = ctx.db.get_mobile_data(&mob_id).expect("get").expect("mob");
        let cancelled = fire_mobile_dg_triggers(
            &ctx.db,
            &ctx.connections,
            &mob,
            crate::MobileTriggerType::OnCommand,
            "",
            "pan",
            "pan",
            "gold",
        );
        assert!(!cancelled, "test triggers don't return 0");
        // `fired` global must be 'pan', not 'shovel' (or empty).
        assert_eq!(
            ctx.db.get_dg_global("fired").expect("get").as_deref(),
            Some("pan")
        );
    }

    // ---------- F3 (author-stamped DG opcode gate) ----------

    /// Build an `AreaData` with the given owner + AllBuilders permission.
    /// `permission` controls whether the author can edit the area.
    fn make_area(name: &str, owner: Option<&str>, perm: crate::AreaPermission) -> crate::AreaData {
        serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "name": name,
            "prefix": name.to_lowercase(),
            "owner": owner,
            "permission_level": match perm {
                crate::AreaPermission::AllBuilders => "all_builders",
                crate::AreaPermission::OwnerOnly => "owner_only",
                crate::AreaPermission::Trusted => "trusted",
            },
            "trusted_builders": [],
        }))
        .expect("build area")
    }

    /// Returns true when an authored trigger purges a target mob in the
    /// supplied target area. The target mob is created here, the gate is
    /// queried via cmd_purge, and the assertion is "is the mob still in
    /// the db after the trigger ran".
    fn purge_test(
        author_name: &str,
        author_owns_target: bool,
        elevated: bool,
    ) -> bool {
        // Author is a non-admin builder.
        let author_char: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": author_name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
            "is_admin": false,
            "is_builder": true,
        }))
        .expect("build author");
        let mut ctx = make_ctx(SelfKind::Mob, uuid::Uuid::new_v4(), "host");
        ctx.db.save_character_data(author_char).expect("save author");

        // Build two areas. Author owns the first either way; the second
        // is owned by someone else and gated OwnerOnly so the author
        // can NOT edit it.
        let owner_area = make_area("home", Some(author_name), crate::AreaPermission::AllBuilders);
        ctx.db.save_area_data(owner_area.clone()).expect("save home");
        let foreign_area = make_area("foreign", Some("other"), crate::AreaPermission::OwnerOnly);
        ctx.db.save_area_data(foreign_area.clone()).expect("save foreign");

        let target_area_id = if author_owns_target { owner_area.id } else { foreign_area.id };

        // Target mob lives in the chosen area.
        let mut target = crate::types::MobileData::new("victim".to_string());
        target.area_id = Some(target_area_id);
        let target_id = target.id;
        ctx.db.save_mobile_data(target).expect("save target");

        // Set the author + elevation on the eval context (mirrors what
        // fire_mobile_dg_triggers would set from t.authored_by/t.elevated).
        ctx.authored_by = Some(author_name.to_string());
        ctx.elevated = elevated;

        // Fire `purge <target_uuid>` directly — exercises the gate.
        let body = format!("purge {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        // If the mob is gone the gate let it through.
        ctx.db.get_mobile_data(&target_id).expect("get").is_none()
    }

    #[test]
    fn dg_purge_blocked_when_author_lacks_target_area_permission() {
        // Builder authored a trigger; the trigger tries to purge a mob
        // in a foreign OwnerOnly area. Gate must refuse and the mob
        // must survive.
        let purged = purge_test("rogue_builder", /* owns_target */ false, /* elevated */ false);
        assert!(!purged, "DG purge across area boundary must be blocked");
    }

    #[test]
    fn dg_purge_allowed_when_author_owns_target_area() {
        // Same author, but the target lives in their own area. Gate
        // should let the purge through.
        let purged = purge_test("home_builder", /* owns_target */ true, /* elevated */ false);
        assert!(purged, "DG purge within author's area must be allowed");
    }

    #[test]
    fn dg_purge_allowed_when_trigger_is_admin_elevated() {
        // Cross-area purge with `elevated=true` must bypass the gate.
        // This is the admin-set escape hatch for legitimate cross-area
        // automation (city-wide cleanup mobs, etc.).
        let purged = purge_test("rogue_builder", /* owns_target */ false, /* elevated */ true);
        assert!(purged, "elevated triggers must bypass the area gate");
    }

    #[test]
    fn dg_purge_allowed_when_authored_by_is_none() {
        // Importer-seeded triggers (None author) keep their legacy
        // trusted-pass behavior; otherwise importing tbamud would break.
        let mut ctx = make_ctx(SelfKind::Mob, uuid::Uuid::new_v4(), "host");
        let foreign_area = make_area("foreign", Some("other"), crate::AreaPermission::OwnerOnly);
        ctx.db.save_area_data(foreign_area.clone()).expect("save");
        let mut target = crate::types::MobileData::new("victim".to_string());
        target.area_id = Some(foreign_area.id);
        let target_id = target.id;
        ctx.db.save_mobile_data(target).expect("save");

        ctx.authored_by = None;
        ctx.elevated = false;
        let body = format!("purge {tid}\nhalt", tid = target_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);
        assert!(
            ctx.db.get_mobile_data(&target_id).expect("get").is_none(),
            "system-authored triggers (authored_by=None) keep legacy permission"
        );
    }

    // The BUILDER_DEBUG_LOG is process-global static state, so these
    // tests use unique self_name strings to find their own entries
    // amongst whatever else the test runner has pushed.
    fn find_log_entries_for(self_name: &str) -> Vec<String> {
        crate::session::broadcast::get_builder_debug_lines(50)
            .into_iter()
            .filter(|line| line.contains(self_name))
            .collect()
    }

    #[test]
    fn warn_builder_surfaces_unknown_command() {
        let tag = "warnbuilder-unknown-cmd";
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        let body = "totallybogusverb foo bar";
        assert_eq!(fire_dg(body, &ctx), Outcome::Done);

        let entries = find_log_entries_for(tag);
        assert!(
            entries.iter().any(|e| e.contains("unknown command: totallybogusverb")),
            "expected 'unknown command' warning, got entries: {entries:?}"
        );
    }

    #[test]
    fn warn_builder_dedups_consecutive_identical_errors() {
        let tag = "warnbuilder-dedup";
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        let body = "alsogibberish";

        // Fire three times back-to-back — only one entry should land.
        fire_dg(body, &ctx);
        fire_dg(body, &ctx);
        fire_dg(body, &ctx);

        let entries = find_log_entries_for(tag);
        let unknown_cmd_count = entries
            .iter()
            .filter(|e| e.contains("unknown command: alsogibberish"))
            .count();
        assert_eq!(
            unknown_cmd_count, 1,
            "dedup should suppress consecutive duplicates; got entries: {entries:?}"
        );
    }

    #[test]
    fn warn_builder_surfaces_parse_error() {
        let tag = "warnbuilder-parse-error";
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        // Unclosed `if` triggers a parse error.
        let body = "if %actor.level% > 5\n  say hi";
        let _ = fire_dg(body, &ctx);

        let entries = find_log_entries_for(tag);
        assert!(
            entries.iter().any(|e| e.contains("parse error")),
            "expected 'parse error' warning, got entries: {entries:?}"
        );
    }

    #[test]
    fn warn_builder_silent_on_gameplay_state_lookups() {
        // `mkill target_not_in_room` should NOT surface — that's
        // gameplay state, not an author error.
        let tag = "warnbuilder-gameplay-silent";
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        let body = "mkill nobody-by-that-name";
        let _ = fire_dg(body, &ctx);

        let entries = find_log_entries_for(tag);
        assert!(
            entries.is_empty(),
            "gameplay no-op (target not in room) must stay silent; got entries: {entries:?}"
        );
    }

    #[test]
    fn remote_three_arg_form_writes_explicit_value() {
        // IronMUD extension: `remote VAR TARGET VALUE` substitutes
        // VALUE and writes it directly, bypassing the local-of-same-name
        // lookup. Lets builders compute a value in `eval` under one
        // name and remote it under a different name.
        let tag = "remote-3arg";
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "\
eval today 99
remote cookie_day %actor.id% %today%
if %actor.cookie_day% == 99
  return 5
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(5));
        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert_eq!(after.dg_vars.get("cookie_day").map(String::as_str), Some("99"));
    }

    #[test]
    fn remote_two_arg_form_still_resolves_var_locally() {
        // Stock tbamud behavior: `remote VAR TARGET` writes the current
        // value of local VAR. Make sure the 3-arg extension didn't
        // regress this.
        let tag = "remote-2arg";
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "\
set score 7
remote score %actor.id%
if %actor.score% == 7
  return 6
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(6));
    }

    #[test]
    fn remote_three_arg_form_accepts_multi_word_value() {
        // The third arg captures everything after the target token, so
        // `remote greeting %actor.id% Welcome back!` writes the full phrase.
        let tag = "remote-3arg-multiword";
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "remote greeting %actor.id% Welcome back!";
        let _ = fire_dg(body, &ctx);
        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert_eq!(
            after.dg_vars.get("greeting").map(String::as_str),
            Some("Welcome back!")
        );
    }

    #[test]
    fn percent_verb_survives_substitution() {
        // Regression: `%send% %actor% msg` must dispatch to cmd_send,
        // not be eaten by vars::substitute as a bare-name lookup of
        // "send" that then leaves the player name as the verb.
        let tag = "percent-verb-pct";
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Obj, Uuid::new_v4(), tag);
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });

        let body = "%send% %actor% You feel a tingle.";
        let _ = fire_dg(body, &ctx);

        let entries = find_log_entries_for(tag);
        assert!(
            !entries.iter().any(|e| e.contains("unknown command")),
            "expected no 'unknown command' warning, got: {entries:?}"
        );
    }

    #[test]
    fn warn_builder_surfaces_remote_empty_var_name() {
        let tag = "warnbuilder-remote-emptyvar";
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), tag);
        // `remote` with no var name token.
        let body = "remote";
        let _ = fire_dg(body, &ctx);

        let entries = find_log_entries_for(tag);
        assert!(
            entries.iter().any(|e| e.contains("remote: missing variable name")),
            "expected 'remote: missing variable name', got entries: {entries:?}"
        );
    }

    // ---------- Part 1: equipped accessors ----------

    /// Helper: build + save an item with a vnum and immediately equip it on
    /// the named player. Mirrors the wear/grant setup used by tests in
    /// tests/items_affects.rs.
    fn equip_item_with_vnum(db: &Db, char_name: &str, vnum: &str, name: &str) -> Uuid {
        let mut item = crate::types::ItemData::new(
            name.to_string(),
            format!("a {name}"),
            format!("A {name} lies here."),
        );
        item.vnum = Some(vnum.to_string());
        let item_id = item.id;
        db.save_item_data(item).expect("save item");
        db.move_item_to_equipped(&item_id, char_name).expect("equip");
        item_id
    }

    #[test]
    fn actor_equipped_vnum_counts_equipped_items() {
        // Equip 2 items with vnum=3010 and 1 item with vnum=3020 on a PC,
        // then verify %actor.equipped(3010)% returns "2" and
        // %actor.equipped(3020)% returns "1". The vnum-miss case
        // (%actor.equipped(9999)%) returns "0".
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "setwearer",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");

        equip_item_with_vnum(&ctx.db, "setwearer", "3010", "left glove");
        equip_item_with_vnum(&ctx.db, "setwearer", "3010", "right glove");
        equip_item_with_vnum(&ctx.db, "setwearer", "3020", "helmet");

        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "setwearer".to_string(),
        });

        // Count of 2 → return 22, count of 1 → return 11, count of 0 → return 0.
        let body = "\
if %actor.equipped(3010)% == 2
  if %actor.equipped(3020)% == 1
    if %actor.equipped(9999)% == 0
      return 42
    end
  end
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(42));
    }

    #[test]
    fn actor_equipped_bare_returns_comma_list() {
        // Bare %actor.equipped% returns a comma-joined list of equipped
        // item names. Mirror of %actor.inventory% (collection_actor_field).
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "listwearer",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");

        equip_item_with_vnum(&ctx.db, "listwearer", "1", "crown");
        equip_item_with_vnum(&ctx.db, "listwearer", "2", "cloak");

        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "listwearer".to_string(),
        });

        // Stash the list in a local, verify it's non-empty by direct compare.
        // (Both items present → "crown, cloak" or "cloak, crown" depending on
        // iteration order; we just assert the local has at least one comma.)
        let body = "\
set worn %actor.equipped%
if %worn% == crown, cloak
  return 19
end
if %worn% == cloak, crown
  return 19
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(19));
    }

    // ---------- Part 3: proto edit + auto-refresh ----------

    /// Helper: create an item-kind proto, attach it to two fresh items,
    /// return (db handle, proto_vnum, [item ids]). Both items end up with
    /// triggers carrying source_proto_vnum tagged to the proto.
    fn setup_two_attached_items(
        ctx: &EvalCtx,
        proto_vnum: &str,
        flags: &str,
        body: &str,
    ) -> Vec<Uuid> {
        let proto = crate::types::DgTriggerProto {
            vnum: proto_vnum.to_string(),
            name: "set_check".to_string(),
            attach_kind: crate::types::DgAttachKind::Obj,
            flags: flags.to_string(),
            numeric_arg: 100,
            arglist: String::new(),
            body: body.to_string(),
        };
        ctx.db.save_dg_trigger_proto(&proto).expect("save proto");

        let mut ids = Vec::new();
        for n in 0..2 {
            let item = crate::types::ItemData::new(
                format!("glove_{n}"),
                format!("a glove_{n}"),
                format!("A glove_{n} lies here."),
            );
            let id = item.id;
            ctx.db.save_item_data(item).expect("save item");
            ids.push(id);
        }
        // Attach via the same cmds path the runtime uses.
        for id in &ids {
            crate::script::dg::cmds::attach_trigger_proto(
                proto_vnum,
                &id.to_string(),
                ctx,
            );
        }
        ids
    }

    #[test]
    fn proto_edit_refreshes_attached_instances() {
        // Attach proto to two items, edit body via save_with_refresh,
        // verify both items' attached triggers reflect the new body.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ids = setup_two_attached_items(&ctx, "7777", "g", "halt");

        // Each item should now have one OnGet trigger backed by the proto.
        for id in &ids {
            let item = ctx.db.get_item_data(id).expect("get").expect("item");
            assert_eq!(item.triggers.len(), 1);
            assert_eq!(item.triggers[0].dg_body.as_deref(), Some("halt"));
            assert_eq!(item.triggers[0].source_proto_vnum.as_deref(), Some("7777"));
        }

        // Edit the proto body via the save+refresh path.
        let mut proto = ctx.db.get_dg_trigger_proto("7777").unwrap().unwrap();
        proto.body = "return 0".to_string();
        let (refreshed, warnings) = ctx
            .db
            .save_dg_trigger_proto_with_refresh(&proto)
            .expect("save");
        assert_eq!(refreshed, 2, "both attached items should be refreshed");
        assert!(warnings.is_empty(), "halt+return 0 has no warnings");

        // Both attached triggers now carry the new body.
        for id in &ids {
            let item = ctx.db.get_item_data(id).expect("get").expect("item");
            assert_eq!(item.triggers.len(), 1);
            assert_eq!(item.triggers[0].dg_body.as_deref(), Some("return 0"));
        }
    }

    #[test]
    fn proto_flag_change_rebuilds_trigger_types() {
        // Proto flags `g` → `gh` (OnGet + OnDrop). Attached items should
        // gain a second trigger of the new type on refresh.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ids = setup_two_attached_items(&ctx, "7700", "g", "halt");

        for id in &ids {
            let item = ctx.db.get_item_data(id).expect("get").expect("item");
            assert_eq!(item.triggers.len(), 1);
            assert_eq!(item.triggers[0].trigger_type, crate::types::ItemTriggerType::OnGet);
        }

        let mut proto = ctx.db.get_dg_trigger_proto("7700").unwrap().unwrap();
        proto.flags = "gh".to_string();
        let (refreshed, _) = ctx
            .db
            .save_dg_trigger_proto_with_refresh(&proto)
            .expect("save");
        assert_eq!(refreshed, 2);

        for id in &ids {
            let item = ctx.db.get_item_data(id).expect("get").expect("item");
            let types: Vec<_> = item.triggers.iter().map(|t| t.trigger_type).collect();
            assert!(types.contains(&crate::types::ItemTriggerType::OnGet));
            assert!(types.contains(&crate::types::ItemTriggerType::OnDrop));
            assert_eq!(item.triggers.len(), 2);
        }
    }

    #[test]
    fn proto_parse_error_aborts_save() {
        // Malformed body fails the analyzer's ParseError check. Save must
        // be refused and attached instances unchanged.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ids = setup_two_attached_items(&ctx, "7766", "g", "halt");

        let mut proto = ctx.db.get_dg_trigger_proto("7766").unwrap().unwrap();
        proto.body = "if 1 == 1\n  halt".to_string(); // missing `end`
        let result = ctx.db.save_dg_trigger_proto_with_refresh(&proto);
        assert!(result.is_err(), "malformed body should be refused");

        // Persisted proto body unchanged on disk.
        let persisted = ctx.db.get_dg_trigger_proto("7766").unwrap().unwrap();
        assert_eq!(persisted.body, "halt");

        // Attached triggers also unchanged.
        for id in &ids {
            let item = ctx.db.get_item_data(id).expect("get").expect("item");
            assert_eq!(item.triggers[0].dg_body.as_deref(), Some("halt"));
        }
    }

    #[test]
    fn proto_delete_orphans_attached() {
        // After delete, attached instances keep their bodies but lose
        // source_proto_vnum (so future proto edits don't touch them).
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ids = setup_two_attached_items(&ctx, "7755", "g", "halt");

        let orphaned = ctx
            .db
            .orphan_attached_dg_triggers("7755")
            .expect("orphan sweep");
        assert_eq!(orphaned, 2);
        ctx.db.delete_dg_trigger_proto("7755").expect("delete");

        // Bodies preserved, links cleared.
        for id in &ids {
            let item = ctx.db.get_item_data(id).expect("get").expect("item");
            assert_eq!(item.triggers.len(), 1);
            assert_eq!(item.triggers[0].dg_body.as_deref(), Some("halt"));
            assert!(item.triggers[0].source_proto_vnum.is_none());
        }
        assert!(ctx.db.get_dg_trigger_proto("7755").unwrap().is_none());
    }

    #[test]
    fn legacy_dg_name_detach_still_works() {
        // Pre-existing triggers without source_proto_vnum (legacy
        // un-tagged shape) still match the apply_detach fallback path
        // by dg_name. Mirror of dg_detach_removes_attached_trigger but
        // exercising the explicit "untagged" instance shape.
        let mut host = crate::types::MobileData::new("legacy_host".to_string());
        host.triggers.push(crate::MobileTrigger {
            trigger_type: crate::MobileTriggerType::OnGreet,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: Vec::new(),
            interval_secs: 60,
            last_fired: 0,
            dg_body: Some("halt".to_string()),
            dg_name: Some("legacy".to_string()),
            authored_by: None,
            elevated: false,
            // Untagged — the legacy fallback.
            source_proto_vnum: None,
        });
        let host_id = host.id;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "self_mob");
        ctx.db.save_mobile_data(host).expect("save");

        let proto = crate::types::DgTriggerProto {
            vnum: "7744".to_string(),
            name: "legacy".to_string(),
            attach_kind: crate::types::DgAttachKind::Mob,
            flags: "g".to_string(),
            numeric_arg: 100,
            arglist: String::new(),
            body: "halt".to_string(),
        };
        ctx.db.save_dg_trigger_proto(&proto).expect("save proto");

        let body = format!("detach 7744 {hid}\nhalt", hid = host_id);
        assert_eq!(fire_dg(&body, &ctx), Outcome::Halt);

        let saved = ctx.db.get_mobile_data(&host_id).unwrap().unwrap();
        assert_eq!(saved.triggers.len(), 0, "legacy dg_name match still detaches");
    }

    #[test]
    fn edit_attached_instance_routes_through_proto() {
        // The Rhai-side editor uses get_*_trigger_source_proto to detect
        // tagged instances; verify the underlying accessor returns the
        // bound vnum after attach and clears after detach.
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ids = setup_two_attached_items(&ctx, "7733", "g", "halt");

        let item = ctx.db.get_item_data(&ids[0]).unwrap().unwrap();
        assert_eq!(item.triggers[0].source_proto_vnum.as_deref(), Some("7733"));
    }

    // ---------- Part 2: slot-aware eq(slot) ----------

    /// Helper: equip an item with explicit wear_locations on the named
    /// player at a chosen slot. Uses move_item_to_equipped_at to set
    /// `currently_worn_at` deterministically.
    fn equip_item_at_slot(
        db: &Db,
        char_name: &str,
        vnum: &str,
        name: &str,
        wear_locations: Vec<crate::types::WearLocation>,
        slot: crate::types::WearLocation,
    ) -> Uuid {
        let mut item = crate::types::ItemData::new(
            name.to_string(),
            format!("a {name}"),
            format!("A {name} lies here."),
        );
        item.vnum = Some(vnum.to_string());
        item.wear_locations = wear_locations;
        let item_id = item.id;
        db.save_item_data(item).expect("save item");
        db.move_item_to_equipped_at(&item_id, char_name, Some(slot)).expect("equip");
        item_id
    }

    #[test]
    fn eq_returns_item_in_specific_slot() {
        use crate::types::WearLocation;
        // Two gloves with identical wear_locations but worn in distinct
        // slots — slot-aware eq() must distinguish them by
        // currently_worn_at, not by item name or order.
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "twohands",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");

        equip_item_at_slot(
            &ctx.db,
            "twohands",
            "9001",
            "leftglove",
            vec![WearLocation::LeftHand, WearLocation::RightHand],
            WearLocation::LeftHand,
        );
        equip_item_at_slot(
            &ctx.db,
            "twohands",
            "9002",
            "rightglove",
            vec![WearLocation::LeftHand, WearLocation::RightHand],
            WearLocation::RightHand,
        );

        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "twohands".to_string(),
        });

        let body = "\
set l %actor.eq(left_hand)%
set r %actor.eq(right_hand)%
if %l% == leftglove
  if %r% == rightglove
    return 77
  end
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(77));
    }

    #[test]
    fn eq_with_unknown_slot_falls_back_to_first() {
        // Stock idiom: `if %actor.eq(*)%` expects truthy "something is
        // equipped" boolean — unknown/asterisk arg must keep returning
        // the first equipped item's name so existing scripts don't break.
        use crate::types::WearLocation;
        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "anyhands",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        equip_item_at_slot(
            &ctx.db,
            "anyhands",
            "9010",
            "ring",
            vec![WearLocation::FingerLeft],
            WearLocation::FingerLeft,
        );

        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "anyhands".to_string(),
        });
        // Asterisk falls through to first-equipped.
        let body = "\
if %actor.eq(*)% == ring
  return 5
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(5));
    }

    #[test]
    fn currently_worn_at_clears_on_remove() {
        // Equip → record slot → move to inventory → verify the slot
        // marker clears so subsequent eq(slot) queries return empty.
        use crate::types::WearLocation;
        let ctx = make_ctx(SelfKind::Mob, Uuid::new_v4(), "host");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "shifty",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");

        let item_id = equip_item_at_slot(
            &ctx.db,
            "shifty",
            "9020",
            "amulet",
            vec![WearLocation::Neck],
            WearLocation::Neck,
        );

        let equipped = ctx.db.get_item_data(&item_id).unwrap().unwrap();
        assert_eq!(equipped.currently_worn_at, Some(WearLocation::Neck));

        // Move to inventory and verify the slot marker cleared.
        ctx.db.move_item_to_inventory(&item_id, "shifty").unwrap();
        let after = ctx.db.get_item_data(&item_id).unwrap().unwrap();
        assert!(after.currently_worn_at.is_none(), "slot must clear on remove");
    }

    #[test]
    fn self_equipped_works_on_mob() {
        // Mob path: %self.equipped(vnum)% and bare %self.equipped% read
        // from get_items_equipped_on_mobile. Exercises both the bare
        // collection (resolve_self_field) and the count form
        // (read_actor_call via self → actor coercion).
        let mob = crate::types::MobileData::new("guard".to_string());
        let mob_id = mob.id;

        let ctx = make_ctx(SelfKind::Mob, mob_id, "guard");
        ctx.db.save_mobile_data(mob).expect("save mob");

        // Equip two items vnum=5500 on the mob via direct location set
        // (move_item_to_equipped is char-only; mob equips go through a
        // different db path that's not yet on the test surface).
        for name in ["sword", "buckler"] {
            let mut item = crate::types::ItemData::new(
                name.to_string(),
                format!("a {name}"),
                format!("A {name} lies here."),
            );
            item.vnum = Some("5500".to_string());
            item.location = crate::types::ItemLocation::Equipped(mob_id.to_string());
            ctx.db.save_item_data(item).expect("save");
        }

        // %self.equipped(5500)% == 2 (count form), %self.equipped% (bare)
        // joins the two item names with ", " in either order.
        let body = "\
if %self.equipped(5500)% == 2
  set worn %self.equipped%
  if %worn% == sword, buckler
    return 33
  end
  if %worn% == buckler, sword
    return 33
  end
end";
        assert_eq!(fire_dg(body, &ctx), Outcome::Return(33));
    }

    #[test]
    fn dg_award_achievement_unlocks_manual_criterion_for_actor() {
        use crate::types::{
            AchievementCategory, AchievementCriterion, AchievementDef, AchievementReward, AchievementSource,
        };

        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Obj, Uuid::new_v4(), "sword of heroes");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });
        ctx.db
            .save_achievement(AchievementDef {
                key: "first_blood".to_string(),
                name: "First Blood".to_string(),
                description: "Wielded the legendary sword.".to_string(),
                category: AchievementCategory::Combat,
                criterion: AchievementCriterion::Manual,
                reward: AchievementReward::default(),
                hidden: false,
                source: AchievementSource::default(),
            })
            .expect("save def");

        let body = "award_achievement %actor.name% first_blood\nhalt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);

        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert!(
            after.achievements_unlocked.contains_key("first_blood"),
            "DG award_achievement must unlock the key on the actor"
        );
    }

    #[test]
    fn dg_award_achievement_refuses_engine_criterion() {
        // Engine-criterion (Counter) achievements must not be shortcut
        // from DG — they're reserved for the engine's notify path.
        use crate::types::{
            AchievementCategory, AchievementCriterion, AchievementDef, AchievementReward, AchievementSource,
        };

        let cid = Uuid::new_v4();
        let mut ctx = make_ctx(SelfKind::Obj, Uuid::new_v4(), "sword");
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "alex",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ctx.db.save_character_data(ch).expect("save");
        ctx.actor = Some(ActorRef::Player {
            connection_id: cid.to_string(),
            char_id: cid,
            name: "alex".to_string(),
        });
        ctx.db
            .save_achievement(AchievementDef {
                key: "ten_kills".to_string(),
                name: "Ten Kills".to_string(),
                description: "".to_string(),
                category: AchievementCategory::Combat,
                criterion: AchievementCriterion::Counter {
                    counter: "kills.any".to_string(),
                    threshold: 10,
                },
                reward: AchievementReward::default(),
                hidden: false,
                source: AchievementSource::default(),
            })
            .expect("save def");

        let body = "award_achievement %actor.name% ten_kills\nhalt";
        assert_eq!(fire_dg(body, &ctx), Outcome::Halt);

        let after = ctx.db.get_character_data("alex").unwrap().unwrap();
        assert!(
            !after.achievements_unlocked.contains_key("ten_kills"),
            "engine-criterion keys must not unlock via DG award"
        );
    }
}
