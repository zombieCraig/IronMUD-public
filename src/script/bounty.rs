//! Rhai bindings for the bounty board.
//!
//! Same split as the rest of the builder tier: the rules live in
//! `crate::bounty`, the words live in `scripts/commands/bounty.rhai`. Every
//! lifecycle call returns `#{ok, message}` rather than raising, because a
//! refusal is a normal outcome here — claiming something somebody else already
//! took is not an error, it is a fact the builder needs told.

use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;

use crate::bounty;
use crate::db::Db;
use crate::types::{BuildRequest, ContentKind, RequestStatus};
use crate::{SharedConnections, SharedState};

fn request_map(r: &BuildRequest) -> Map {
    let mut m = Map::new();
    m.insert("found".into(), Dynamic::from(true));
    m.insert("ticket".into(), Dynamic::from(r.ticket_number));
    m.insert("requester".into(), Dynamic::from(r.requester.clone()));
    m.insert("origin".into(), Dynamic::from(r.origin.key().to_string()));
    m.insert("system".into(), Dynamic::from(r.requester == bounty::SYSTEM_REQUESTER));
    m.insert(
        "kind".into(),
        Dynamic::from(r.kind.map(|k| k.key()).unwrap_or("any").to_string()),
    );
    m.insert("area".into(), Dynamic::from(r.area_label.clone()));
    m.insert("title".into(), Dynamic::from(r.title.clone()));
    m.insert("detail".into(), Dynamic::from(r.detail.clone()));
    m.insert("points".into(), Dynamic::from(r.points as i64));
    m.insert("status".into(), Dynamic::from(r.status.key().to_string()));
    m.insert("live".into(), Dynamic::from(r.status.is_live()));
    m.insert(
        "claimed_by".into(),
        Dynamic::from(r.claimed_by.clone().unwrap_or_default()),
    );
    m.insert(
        "fulfilled_by".into(),
        Dynamic::from(r.fulfilled_by.clone().unwrap_or_default()),
    );
    m.insert("target".into(), Dynamic::from(r.target.clone()));
    let linked: Array = r.linked.iter().cloned().map(Dynamic::from).collect();
    m.insert("linked".into(), Dynamic::from(linked));
    let notes: Array = r
        .notes
        .iter()
        .map(|n| {
            let mut row = Map::new();
            row.insert("author".into(), Dynamic::from(n.author.clone()));
            row.insert("message".into(), Dynamic::from(n.message.clone()));
            row.insert("at".into(), Dynamic::from(n.created_at));
            Dynamic::from(row)
        })
        .collect();
    m.insert("notes".into(), Dynamic::from(notes));
    m.insert("created_at".into(), Dynamic::from(r.created_at));
    m
}

fn outcome_map(o: bounty::Outcome) -> Map {
    let mut m = Map::new();
    m.insert("ok".into(), Dynamic::from(o.is_ok()));
    m.insert("message".into(), Dynamic::from(o.message().to_string()));
    m
}

fn failed(message: &str) -> Map {
    let mut m = Map::new();
    m.insert("ok".into(), Dynamic::from(false));
    m.insert("message".into(), Dynamic::from(message.to_string()));
    m
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // list_bounties(filter) -> Array
    //
    // filter is "" (everything live), a status key, or "mine:<name>" for the
    // ones a builder has claimed. Closed requests are kept as a record and are
    // only listed when asked for by status.
    let cloned_db = db.clone();
    engine.register_fn("list_bounties", move |filter: String| -> Array {
        let Ok(all) = cloned_db.list_build_requests() else {
            return Array::new();
        };
        let f = filter.trim().to_lowercase();
        let mine = f.strip_prefix("mine:").map(|n| n.to_string());
        let status = RequestStatus::from_key(&f);

        let mut rows: Vec<&BuildRequest> = all
            .iter()
            .filter(|r| match (&mine, status) {
                (Some(name), _) => r.claimed_by.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(name)),
                (None, Some(s)) => r.status == s,
                (None, None) => r.status.is_live(),
            })
            .collect();

        // Open work first, then by value: the board is a list of things to do,
        // and the things already being done belong underneath them.
        rows.sort_by_key(|r| {
            (
                match r.status {
                    RequestStatus::Open => 0,
                    RequestStatus::Claimed => 1,
                    RequestStatus::Submitted => 2,
                    _ => 3,
                },
                -r.points,
                r.ticket_number,
            )
        });
        rows.into_iter().map(|r| Dynamic::from(request_map(r))).collect()
    });

    // get_bounty(ticket) -> Map
    let cloned_db = db.clone();
    engine.register_fn("get_bounty", move |ticket: i64| -> Map {
        match cloned_db.get_build_request_by_ticket(ticket) {
            Ok(Some(r)) => request_map(&r),
            _ => {
                let mut m = Map::new();
                m.insert("found".into(), Dynamic::from(false));
                m
            }
        }
    });

    // post_bounty(requester, kind, area_label, title, detail, points) -> Map
    //
    // `area_label` is an area prefix, or "" for a request that is not about a
    // particular area. An unknown prefix is not an error — the request is
    // simply unfiled, which is better than refusing to record what somebody
    // wants.
    let cloned_db = db.clone();
    engine.register_fn(
        "post_bounty",
        move |requester: String, kind: String, area_label: String, title: String, detail: String, points: i64| -> Map {
            if title.trim().is_empty() {
                return failed("A bounty needs a title.");
            }
            let kind = ContentKind::from_key(&kind);
            let area = cloned_db.list_all_areas().ok().and_then(|areas| {
                areas
                    .into_iter()
                    .find(|a| a.prefix.eq_ignore_ascii_case(area_label.trim()))
            });
            let (area_id, label) = match area {
                Some(a) => (Some(a.id), a.prefix),
                None => (None, String::new()),
            };
            match bounty::post(
                &cloned_db,
                &requester,
                kind,
                area_id,
                &label,
                &title,
                &detail,
                points as i32,
                now(),
            ) {
                Ok(r) => {
                    let mut m = outcome_map(bounty::Outcome::Ok);
                    m.insert("ticket".into(), Dynamic::from(r.ticket_number));
                    m
                }
                Err(e) => {
                    // Not leaked into a &'static str: the message is only ever
                    // shown once, and a cold error path is a poor reason to
                    // grow the binary's static data forever.
                    tracing::warn!("posting a bounty failed: {e}");
                    failed("The bounty could not be recorded.")
                }
            }
        },
    );

    let cloned_db = db.clone();
    engine.register_fn("claim_bounty", move |ticket: i64, claimant: String| -> Map {
        match bounty::claim(&cloned_db, ticket, &claimant, now()) {
            Ok(o) => outcome_map(o),
            Err(_) => failed("The board could not be read."),
        }
    });

    let cloned_db = db.clone();
    engine.register_fn(
        "drop_bounty",
        move |ticket: i64, actor: String, is_admin: bool| -> Map {
            match bounty::drop_claim(&cloned_db, ticket, &actor, is_admin, now()) {
                Ok(o) => outcome_map(o),
                Err(_) => failed("The board could not be read."),
            }
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "submit_bounty",
        move |ticket: i64, claimant: String, linked: String| -> Map {
            let linked: Vec<String> = linked
                .split(|c| c == ',' || c == ' ')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            match bounty::submit(&cloned_db, ticket, &claimant, linked, now()) {
                Ok(o) => outcome_map(o),
                Err(_) => failed("The board could not be read."),
            }
        },
    );

    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "accept_bounty",
        move |ticket: i64, judge: String, is_admin: bool| -> Map {
            match bounty::accept(
                &cloned_db,
                &cloned_conns,
                &cloned_state,
                ticket,
                &judge,
                is_admin,
                now(),
            ) {
                Ok(o) => outcome_map(o),
                Err(_) => failed("The board could not be read."),
            }
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "reject_bounty",
        move |ticket: i64, judge: String, is_admin: bool, reason: String| -> Map {
            match bounty::reject(&cloned_db, ticket, &judge, is_admin, &reason, now()) {
                Ok(o) => outcome_map(o),
                Err(_) => failed("The board could not be read."),
            }
        },
    );
}
