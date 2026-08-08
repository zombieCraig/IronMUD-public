//! The bounty board: posting, claiming, paying, and generating work.
//!
//! `crate::types::build_request` holds the shape. This holds the rules.
//!
//! Two halves that look unrelated and are not:
//!
//! * **The hand-posted half** is a builder saying "I need a blacksmith with a
//!   dialogue tree" and somebody else picking it up. It is the only place in
//!   the whole builder tier where two people cooperate.
//! * **The generated half** is the auditor turning its own findings into work.
//!   A board only builders post to is empty on the day it is most needed, and
//!   an empty board teaches people not to look at it.
//!
//! The generated half is what makes this *guided* rather than merely *scored*,
//! and the property that keeps it honest is that those requests **close
//! themselves**: fixing the underlying problem clears the request whether or
//! not anybody ever read the board. Without that they accumulate into a stale
//! to-do list, which is how every bug tracker dies.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::audit::{Severity, scan::WorldSnapshot};
use crate::db::Db;
use crate::types::{AdminNote, BuildRequest, ContentKind, RequestOrigin, RequestStatus};

/// How long a claim holds before the work returns to the board.
///
/// Three days. Long enough that a builder can claim something on a Friday and
/// finish it on a Sunday; short enough that the board cannot be squatted, which
/// is the failure mode of every claim system that has no expiry at all.
pub const CLAIM_EXPIRY_SECS: i64 = 3 * 24 * 60 * 60;

/// The most auditor-generated requests any one area may hold.
///
/// One badly broken area can emit hundreds of findings. Without a cap the
/// board becomes that area's audit output, and every other request on it is
/// invisible — which is the same failure as having no board.
pub const AUDITOR_CAP_PER_AREA: usize = 8;

/// Total auditor-generated requests on the board at once.
pub const AUDITOR_CAP_TOTAL: usize = 40;

/// The name auditor-generated requests are posted under.
pub const SYSTEM_REQUESTER: &str = "SYSTEM";

/// What a finding is worth, before the kind multiplier.
fn severity_points(sev: Severity) -> i32 {
    match sev {
        Severity::Blocker => 25,
        Severity::Warn => 10,
        Severity::Polish => 4,
    }
}

fn kind_multiplier(kind: Option<ContentKind>) -> f32 {
    match kind {
        Some(ContentKind::Area) => 2.0,
        Some(ContentKind::Quest) => 1.6,
        Some(ContentKind::Mobile) => 1.2,
        _ => 1.0,
    }
}

// ===========================================================================
// Lifecycle
// ===========================================================================

/// Post a request by hand.
pub fn post(
    db: &Db,
    requester: &str,
    kind: Option<ContentKind>,
    area_id: Option<Uuid>,
    area_label: &str,
    title: &str,
    detail: &str,
    points: i32,
    now: i64,
) -> Result<BuildRequest> {
    let request = BuildRequest {
        id: Uuid::new_v4(),
        ticket_number: db.next_build_request_ticket()?,
        requester: requester.to_string(),
        origin: RequestOrigin::Builder,
        kind,
        area_id,
        area_label: area_label.to_string(),
        title: title.trim().to_string(),
        detail: detail.trim().to_string(),
        points: points.clamp(1, 500),
        status: RequestStatus::Open,
        claimed_by: None,
        claimed_at: 0,
        fulfilled_by: None,
        linked: Vec::new(),
        finding_code: None,
        target: String::new(),
        notes: Vec::new(),
        created_at: now,
        updated_at: now,
        closed_at: 0,
    };
    db.save_build_request(&request)?;
    Ok(request)
}

/// The outcome of a lifecycle call, so callers can say what happened without
/// re-deriving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    NotFound,
    /// Wrong status for this action.
    WrongState(&'static str),
    /// The actor is not allowed to do this.
    Denied(&'static str),
}

impl Outcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, Outcome::Ok)
    }

    pub fn message(&self) -> &'static str {
        match self {
            Outcome::Ok => "Done.",
            Outcome::NotFound => "No bounty by that number.",
            Outcome::WrongState(m) => m,
            Outcome::Denied(m) => m,
        }
    }
}

pub fn claim(db: &Db, ticket: i64, claimant: &str, now: i64) -> Result<Outcome> {
    let Some(mut r) = db.get_build_request_by_ticket(ticket)? else {
        return Ok(Outcome::NotFound);
    };
    if !r.can_claim() {
        return Ok(Outcome::WrongState("That bounty is not open."));
    }
    r.status = RequestStatus::Claimed;
    r.claimed_by = Some(claimant.to_string());
    r.claimed_at = now;
    r.updated_at = now;
    db.save_build_request(&r)?;
    Ok(Outcome::Ok)
}

/// Give up a claim. Also what expiry does, without the actor check.
pub fn drop_claim(db: &Db, ticket: i64, actor: &str, is_admin: bool, now: i64) -> Result<Outcome> {
    let Some(mut r) = db.get_build_request_by_ticket(ticket)? else {
        return Ok(Outcome::NotFound);
    };
    if r.status != RequestStatus::Claimed && r.status != RequestStatus::Submitted {
        return Ok(Outcome::WrongState("Nobody has claimed that bounty."));
    }
    let mine = r.claimed_by.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(actor));
    if !mine && !is_admin {
        return Ok(Outcome::Denied("That is not your claim."));
    }
    reopen(&mut r, now);
    db.save_build_request(&r)?;
    Ok(Outcome::Ok)
}

/// Return a request to the board, clearing everything the previous attempt
/// left on it.
///
/// `fulfilled_by` and `linked` go too. Dropping a *submitted* request is one of
/// the two ways out of `Submitted` — `reject` is the other, and it has always
/// cleared them — and leaving them behind renders open work as "Submitted by
/// Bo" on the board.
fn reopen(r: &mut BuildRequest, now: i64) {
    r.status = RequestStatus::Open;
    r.claimed_by = None;
    r.claimed_at = 0;
    r.fulfilled_by = None;
    r.linked.clear();
    r.updated_at = now;
}

pub fn submit(db: &Db, ticket: i64, claimant: &str, linked: Vec<String>, now: i64) -> Result<Outcome> {
    let Some(mut r) = db.get_build_request_by_ticket(ticket)? else {
        return Ok(Outcome::NotFound);
    };
    if r.status != RequestStatus::Claimed {
        return Ok(Outcome::WrongState("That bounty is not claimed."));
    }
    if !r
        .claimed_by
        .as_deref()
        .is_some_and(|n| n.eq_ignore_ascii_case(claimant))
    {
        return Ok(Outcome::Denied("That is not your claim."));
    }
    r.status = RequestStatus::Submitted;
    r.fulfilled_by = r.claimed_by.clone();
    r.linked = linked;
    r.updated_at = now;
    db.save_build_request(&r)?;
    Ok(Outcome::Ok)
}

/// Accept and pay.
///
/// Payment goes through [`crate::script::achievements::apply_to_character`],
/// the mandatory primitive for touching a character out of band — a plain
/// `save_character_data` on an online builder is reverted by the regen tick.
pub fn accept(
    db: &Db,
    connections: &crate::SharedConnections,
    state: &crate::SharedState,
    ticket: i64,
    judge: &str,
    is_admin: bool,
    now: i64,
) -> Result<Outcome> {
    let Some(mut r) = db.get_build_request_by_ticket(ticket)? else {
        return Ok(Outcome::NotFound);
    };
    if r.status != RequestStatus::Submitted {
        return Ok(Outcome::WrongState("That bounty has not been submitted."));
    }
    if !r.can_judge(judge, is_admin) {
        return Ok(Outcome::Denied("Only the requester or an admin can accept that."));
    }
    let Some(payee) = r.fulfilled_by.clone() else {
        return Ok(Outcome::WrongState("Nobody is credited with that work."));
    };
    // A builder may post work and then do it themselves — that is real work,
    // and blocking it would only mean they do it without telling anyone. What
    // nobody may do is *sign off* on their own work, because posting,
    // claiming, submitting and accepting are otherwise a closed loop that pays
    // out forever.
    //
    // Deliberately not exempt for admins. On most servers a builder *is* an
    // admin, so an admin exemption would leave the loop wide open for exactly
    // the people who can reach it. Payment needs a second person; that is the
    // whole guarantee.
    if payee.eq_ignore_ascii_case(judge) {
        return Ok(Outcome::Denied(
            "You cannot sign off on your own work — ask another builder or an admin to accept it.",
        ));
    }

    r.status = RequestStatus::Accepted;
    r.updated_at = now;
    r.closed_at = now;
    db.save_build_request(&r)?;

    let points = r.points;
    crate::script::achievements::apply_to_character(db, connections, &payee, |ch| {
        ch.builder_bounty_points = ch.builder_bounty_points.saturating_add(points);
        true
    });

    // `build.bounties` is a plain tally — bounties filled is a count of events,
    // not a property of the world, so unlike the other `build.*` counters it is
    // incremented rather than reconciled.
    crate::script::achievements::notify_counter_core(db, connections, state, &payee, "build.bounties", 1);
    crate::script::achievements::send_to_player(
        connections,
        &payee,
        &format!(
            "\x1b[1;33m*** Bounty #{} accepted: {} builder points. ***\x1b[0m",
            r.ticket_number, points
        ),
    );

    Ok(Outcome::Ok)
}

/// Send it back. Returns to the board rather than dying — the work was still
/// wanted, and the claimant may well be the one to finish it.
pub fn reject(db: &Db, ticket: i64, judge: &str, is_admin: bool, reason: &str, now: i64) -> Result<Outcome> {
    let Some(mut r) = db.get_build_request_by_ticket(ticket)? else {
        return Ok(Outcome::NotFound);
    };
    if r.status != RequestStatus::Submitted {
        return Ok(Outcome::WrongState("That bounty has not been submitted."));
    }
    if !r.can_judge(judge, is_admin) {
        return Ok(Outcome::Denied("Only the requester or an admin can reject that."));
    }
    if !reason.trim().is_empty() {
        r.notes.push(AdminNote {
            author: judge.to_string(),
            message: reason.trim().to_string(),
            created_at: now,
        });
    }
    reopen(&mut r, now);
    db.save_build_request(&r)?;
    Ok(Outcome::Ok)
}

/// Return squatted work to the board. Called from the same tick that
/// regenerates the auditor's requests.
pub fn expire_claims(db: &Db, now: i64) -> Result<usize> {
    let mut expired = 0;
    for mut r in db.list_build_requests()? {
        if r.status == RequestStatus::Claimed && r.claimed_at > 0 && now - r.claimed_at > CLAIM_EXPIRY_SECS {
            reopen(&mut r, now);
            r.notes.push(AdminNote {
                author: SYSTEM_REQUESTER.to_string(),
                message: "Claim expired and the bounty returned to the board.".to_string(),
                created_at: now,
            });
            db.save_build_request(&r)?;
            expired += 1;
        }
    }
    Ok(expired)
}

// ===========================================================================
// Generation
// ===========================================================================

/// A finding worth turning into a request.
struct Candidate {
    code: &'static str,
    severity: Severity,
    kind: Option<ContentKind>,
    area_id: Option<Uuid>,
    area_label: String,
    target: String,
    title: String,
    detail: String,
}

/// Regenerate the auditor's half of the board.
///
/// Two directions, and both matter:
///
/// * **Open** requests whose finding no longer fires are closed. That is what
///   makes fixing a room pay whether or not anybody read the board.
/// * New findings become requests, deduped on `(finding_code, target)` and
///   capped per area and overall.
///
/// A *claimed or submitted* auditor request is never auto-closed. Somebody is
/// working on it, and yanking it out from under them — even because the problem
/// went away — loses the record of who did it.
pub fn regenerate(db: &Db, snapshot: &WorldSnapshot, now: i64) -> Result<(usize, usize)> {
    let candidates = gather(snapshot);
    let live_codes: HashSet<String> = candidates
        .iter()
        .map(|c| BuildRequest::auditor_key(c.code, &c.target))
        .collect();

    let existing = db.list_build_requests()?;
    let mut closed = 0;
    // Rows this pass just resolved. They still read `Open` in `existing`, which
    // was snapshotted above, and counting them against the caps below would
    // spend budget on work that no longer exists — so an area whose findings
    // all clear and are replaced at once would get nothing new until the next
    // tick, which is exactly when the board most needs to be current.
    let mut closed_ids: HashSet<Uuid> = HashSet::new();

    for mut r in existing.iter().cloned() {
        if r.origin != RequestOrigin::Auditor || r.status != RequestStatus::Open {
            continue;
        }
        let Some(key) = r.dedupe_key() else { continue };
        if live_codes.contains(&key) {
            continue;
        }
        r.status = RequestStatus::Accepted;
        r.fulfilled_by = None;
        r.updated_at = now;
        r.closed_at = now;
        r.notes.push(AdminNote {
            author: SYSTEM_REQUESTER.to_string(),
            message: "Resolved: the problem this was raised for no longer occurs.".to_string(),
            created_at: now,
        });
        db.save_build_request(&r)?;
        closed_ids.insert(r.id);
        closed += 1;
    }

    let still_live = |r: &&BuildRequest| r.status.is_live() && !closed_ids.contains(&r.id);

    // Dedupe against everything still on the board, including claimed work, so
    // a request somebody is already doing is not posted twice.
    let taken: HashSet<String> = existing
        .iter()
        .filter(still_live)
        .filter_map(|r| r.dedupe_key())
        .collect();

    let mut per_area: HashMap<Option<Uuid>, usize> = HashMap::new();
    let mut total_live = 0usize;
    for r in existing
        .iter()
        .filter(|r| r.origin == RequestOrigin::Auditor)
        .filter(still_live)
    {
        *per_area.entry(r.area_id).or_insert(0) += 1;
        total_live += 1;
    }

    let mut created = 0;

    for c in candidates {
        if total_live >= AUDITOR_CAP_TOTAL {
            break;
        }
        let key = BuildRequest::auditor_key(c.code, &c.target);
        if taken.contains(&key) {
            continue;
        }
        let area_count = per_area.entry(c.area_id).or_insert(0);
        if *area_count >= AUDITOR_CAP_PER_AREA {
            continue;
        }

        let points = ((severity_points(c.severity) as f32) * kind_multiplier(c.kind)).round() as i32;
        let request = BuildRequest {
            id: Uuid::new_v4(),
            // Allocated one at a time rather than counted up from a single
            // read, so a builder posting by hand mid-tick cannot be handed a
            // number this loop is about to consume.
            ticket_number: db.next_build_request_ticket()?,
            requester: SYSTEM_REQUESTER.to_string(),
            origin: RequestOrigin::Auditor,
            kind: c.kind,
            area_id: c.area_id,
            area_label: c.area_label,
            title: c.title,
            detail: c.detail,
            points: points.max(1),
            status: RequestStatus::Open,
            claimed_by: None,
            claimed_at: 0,
            fulfilled_by: None,
            linked: Vec::new(),
            finding_code: Some(c.code.to_string()),
            target: c.target,
            notes: Vec::new(),
            created_at: now,
            updated_at: now,
            closed_at: 0,
        };
        db.save_build_request(&request)?;
        created += 1;
        *area_count += 1;
        total_live += 1;
    }

    Ok((created, closed))
}

/// Findings worth posting, worst first.
///
/// Blockers and warnings only. Polish findings are suggestions, and a board
/// full of suggestions is a board nobody reads — the same reason the audit
/// output separates them.
fn gather(snapshot: &WorldSnapshot) -> Vec<Candidate> {
    let ctx = snapshot.ctx();
    let mut out: Vec<Candidate> = Vec::new();

    for area in &snapshot.areas {
        let report = crate::audit::scan::scan_area(snapshot, area, ctx);

        for f in &report.own.findings {
            if f.severity > Severity::Warn {
                continue;
            }
            out.push(Candidate {
                code: f.code,
                severity: f.severity,
                kind: Some(ContentKind::Area),
                area_id: Some(area.id),
                area_label: area.prefix.clone(),
                target: area.prefix.clone(),
                title: format!("{}: {}", area.name, f.message),
                detail: format!("Raised by the content auditor against area {}.", area.prefix),
            });
        }

        for entry in &report.entries {
            for f in &entry.grade.findings {
                if f.severity > Severity::Warn {
                    continue;
                }
                out.push(Candidate {
                    code: f.code,
                    severity: f.severity,
                    kind: Some(entry.kind),
                    area_id: Some(area.id),
                    area_label: area.prefix.clone(),
                    target: entry.label.clone(),
                    title: format!("{} {}: {}", entry.kind.key(), entry.label, f.message),
                    detail: format!(
                        "Raised by the content auditor against {} {} ({}) in {}.",
                        entry.kind.key(),
                        entry.label,
                        entry.name,
                        area.prefix
                    ),
                });
            }
        }
    }

    // World-level findings last in the gather, first in the sort: they are the
    // most valuable things on the board and there are never many of them.
    let facts = snapshot.facts();
    for f in &crate::audit::audit_world(&facts).findings {
        if f.severity > Severity::Warn {
            continue;
        }
        out.push(Candidate {
            code: f.code,
            severity: f.severity,
            kind: None,
            area_id: None,
            area_label: String::new(),
            target: "world".to_string(),
            title: f.message.clone(),
            detail: "Raised by the content auditor against the world as a whole.".to_string(),
        });
    }

    // Worst first, so the caps keep the most valuable work rather than
    // whichever area happened to be scanned first — and world findings ahead of
    // area findings of the same severity, because there are never many of them
    // and they are the ones nobody else will raise. Sorting on `target` alone
    // would put "world" after almost every area prefix alphabetically and make
    // world findings the first thing the total cap drops.
    out.sort_by_key(|c| (c.severity, c.target != "world", c.target.clone(), c.code));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_and_kind_both_move_the_payout() {
        assert!(severity_points(Severity::Blocker) > severity_points(Severity::Warn));
        assert!(severity_points(Severity::Warn) > severity_points(Severity::Polish));
        assert!(kind_multiplier(Some(ContentKind::Area)) > kind_multiplier(Some(ContentKind::Room)));
        assert_eq!(kind_multiplier(None), 1.0);
    }

    #[test]
    fn outcome_messages_are_never_empty() {
        for o in [
            Outcome::Ok,
            Outcome::NotFound,
            Outcome::WrongState("x"),
            Outcome::Denied("y"),
        ] {
            assert!(!o.message().is_empty());
        }
        assert!(Outcome::Ok.is_ok());
        assert!(!Outcome::NotFound.is_ok());
    }
}
