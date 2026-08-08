//! Every write to a player's money — carried gold and bank balance.
//!
//! This module exists because the same bug was written twice, four years apart
//! in codebase time, and the second time it was a player market.
//!
//! For anyone online, `session.character` is the source of truth: it is what
//! `get_player_character` hands to scripts, and the thirst, hunger and regen
//! ticks flush it back to the database wholesale
//! (`src/ticks/character.rs`). A DB-only write to an online player is therefore
//! not merely racy — it is **reverted**, deterministically, by the next flush.
//! Worse, any subsequent call that goes through
//! [`crate::script::achievements::apply_to_character`] (which every achievement
//! counter does) saves the stale session copy immediately, so the revert can
//! land inside the same function call.
//!
//! `add_character_gold` got this right and syncs the session. The four bank
//! bindings beside it did not, and neither did the consignment sale — so a
//! consignment purchase refunded itself, and an online seller's proceeds were
//! destroyed. One helper, one rule, and no site left to copy the wrong shape
//! from.
//!
//! **The counter split, because it is not obvious.** `gold.earned` fires here,
//! on a positive change to *carried* gold, because that is where it has always
//! fired and moving it would double-count. `gold.spent` does **not** fire here:
//! `buy`, `rent` and `identify` bump it themselves, and a helper that also
//! bumped it would count every purchase twice. A caller that spends outside
//! those scripts bumps it explicitly — see `crate::script::consignment`.
//!
//! Bank moves fire no counter at all. Moving your own gold between your pocket
//! and your account is not earning it, and a deposit that bumped `gold.earned`
//! would let a player farm the counter by depositing and withdrawing the same
//! coin.

use crate::db::Db;
use crate::script::achievements::{apply_to_character, notify_counter_core, notify_event_core};
use crate::types::CharacterData;
use crate::{SharedConnections, SharedState};

/// Set carried gold to an absolute value. Negative values are clamped to zero
/// rather than refused: this is the admin/scripted setter, and a caller that
/// wanted a refusal should be using [`add_gold`].
pub fn set_gold(db: &Db, connections: &SharedConnections, state: &SharedState, char_name: &str, gold: i64) -> bool {
    let target = gold.clamp(0, i32::MAX as i64) as i32;
    write_gold(db, connections, state, char_name, |_| Some(target))
}

/// Add (or, with a negative `amount`, subtract) carried gold.
///
/// Returns false when the character cannot be found or the change would take
/// them below zero — the balance is left untouched in that case, so a caller
/// can treat false as "the transaction did not happen".
pub fn add_gold(db: &Db, connections: &SharedConnections, state: &SharedState, char_name: &str, amount: i64) -> bool {
    if amount == 0 {
        // Still confirm the character exists, so callers get a consistent
        // "did this name resolve" answer for a no-op amount.
        return apply_to_character(db, connections, char_name, |_| false).is_some();
    }
    write_gold(db, connections, state, char_name, move |ch| {
        let next = ch.gold as i64 + amount;
        (next >= 0).then_some(next as i32)
    })
}

/// The one place carried gold is written.
///
/// `next` returns the new balance, or `None` to refuse the whole change. The
/// high-water mark, the `gold_high_water` event and the `gold.earned` counter
/// all ride here so no caller has to remember them.
fn write_gold<F>(db: &Db, connections: &SharedConnections, state: &SharedState, char_name: &str, next: F) -> bool
where
    F: FnOnce(&CharacterData) -> Option<i32>,
{
    let mut before: i32 = 0;
    let mut crossed_high = false;
    let mut refused = false;

    let after = apply_to_character(db, connections, char_name, |ch| {
        before = ch.gold;
        let Some(target) = next(ch) else {
            refused = true;
            return false;
        };
        if target == ch.gold {
            return false;
        }
        ch.gold = target;
        if target > ch.gold_high_water {
            ch.gold_high_water = target;
            crossed_high = true;
        }
        true
    });

    let Some(ch) = after else {
        return false;
    };
    if refused {
        return false;
    }

    // Both of these write the character out of band, so they run after the
    // save above — the standing ordering rule. Both route through
    // `apply_to_character` themselves, so they pick up the balance just
    // written rather than clobbering it.
    if crossed_high {
        notify_event_core(
            db,
            connections,
            state,
            char_name,
            "gold_high_water",
            &ch.gold.to_string(),
        );
    }
    let gained = ch.gold as i64 - before as i64;
    if gained > 0 {
        notify_counter_core(db, connections, state, char_name, "gold.earned", gained as u32);
    }
    true
}

/// Set the bank balance to an absolute value, clamped at zero.
pub fn set_bank(db: &Db, connections: &SharedConnections, char_name: &str, amount: i64) -> bool {
    let target = amount.max(0);
    apply_to_character(db, connections, char_name, |ch| {
        if ch.bank_gold == target {
            return false;
        }
        ch.bank_gold = target;
        true
    })
    .is_some()
}

/// Add to (or subtract from) the bank balance. False when the character is
/// missing or the change would overdraw the account.
pub fn add_bank(db: &Db, connections: &SharedConnections, char_name: &str, amount: i64) -> bool {
    let mut refused = false;
    let found = apply_to_character(db, connections, char_name, |ch| {
        let next = ch.bank_gold + amount;
        if next < 0 {
            refused = true;
            return false;
        }
        if next == ch.bank_gold {
            return false;
        }
        ch.bank_gold = next;
        true
    })
    .is_some();
    found && !refused
}

/// Move gold from pocket to account.
///
/// One mutation rather than a withdraw-then-deposit pair: two calls could leave
/// the money nowhere if the second one failed, and this is the operation a
/// player watches most closely.
pub fn deposit(db: &Db, connections: &SharedConnections, char_name: &str, amount: i64) -> bool {
    if amount <= 0 {
        return false;
    }
    let mut refused = false;
    let found = apply_to_character(db, connections, char_name, |ch| {
        if (ch.gold as i64) < amount {
            refused = true;
            return false;
        }
        ch.gold -= amount as i32;
        ch.bank_gold += amount;
        true
    })
    .is_some();
    found && !refused
}

/// Move gold from account to pocket. No `gold.earned` bump: it is the player's
/// own money coming back, and counting it would make the counter farmable by
/// moving one coin back and forth.
pub fn withdraw(db: &Db, connections: &SharedConnections, char_name: &str, amount: i64) -> bool {
    if amount <= 0 {
        return false;
    }
    let mut refused = false;
    let found = apply_to_character(db, connections, char_name, |ch| {
        if ch.bank_gold < amount || ch.gold as i64 + amount > i32::MAX as i64 {
            refused = true;
            return false;
        }
        ch.bank_gold -= amount;
        ch.gold += amount as i32;
        true
    })
    .is_some();
    found && !refused
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlayerSession, World};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn temp_db() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn saved(db: &Db, name: &str, gold: i32, bank: i64) -> CharacterData {
        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::new_v4(),
        }))
        .expect("build character");
        ch.gold = gold;
        ch.bank_gold = bank;
        db.save_character_data(ch.clone()).expect("save");
        ch
    }

    /// Put a character online so the session-first path is the one under test.
    fn online(chars: &[CharacterData]) -> SharedConnections {
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        for c in chars {
            let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let (tx_input, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
            let mut session = PlayerSession::new_for_test(tx_client, tx_input);
            session.character = Some(c.clone());
            conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);
            // Dropping the receivers closes the channels and silences every
            // later send; the tests do not read them but the code does write.
            std::mem::forget((rx, rx_in));
        }
        conns
    }

    fn session_gold(conns: &SharedConnections, name: &str) -> (i32, i64) {
        let guard = conns.lock().unwrap();
        guard
            .values()
            .find_map(|s| {
                s.character
                    .as_ref()
                    .filter(|c| c.name.eq_ignore_ascii_case(name))
                    .map(|c| (c.gold, c.bank_gold))
            })
            .expect("character is online")
    }

    fn db_gold(db: &Db, name: &str) -> (i32, i64) {
        let ch = db.get_character_data(name).expect("read").expect("exists");
        (ch.gold, ch.bank_gold)
    }

    /// The whole reason this module exists.
    ///
    /// A wholesale flush of `session.character` is what the thirst, hunger and
    /// regen ticks do every minute. Before this helper, a DB-only write was
    /// silently undone by it — which is how a consignment purchase refunded
    /// itself. The write has to reach the session, not just the database.
    #[test]
    fn a_purchase_survives_a_session_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "buyer", 500, 0);
        let conns = online(&[ch]);
        let state = World::minimal_shared(db.clone(), conns.clone());

        assert!(add_gold(&db, &conns, &state, "buyer", -120));
        assert_eq!(session_gold(&conns, "buyer").0, 380, "the session copy moved");
        assert_eq!(db_gold(&db, "buyer").0, 380, "and so did the database");

        // Exactly what `src/ticks/character.rs` does once a minute.
        flush_session(&db, &conns, "buyer");
        assert_eq!(db_gold(&db, "buyer").0, 380, "the flush does not refund the purchase");
    }

    /// The seller's half of the same bug: proceeds went to `bank_gold` through
    /// a DB-only write, so an *online* seller lost the money a moment later
    /// while an offline one kept it.
    #[test]
    fn banked_proceeds_survive_a_session_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "seller", 0, 40);
        let conns = online(&[ch]);

        assert!(add_bank(&db, &conns, "seller", 90));
        assert_eq!(session_gold(&conns, "seller").1, 130);

        flush_session(&db, &conns, "seller");
        assert_eq!(db_gold(&db, "seller").1, 130, "the credit is not flushed away");
    }

    /// Deposits had the same defect, and were the older instance of it.
    #[test]
    fn a_deposit_survives_a_session_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "saver", 300, 0);
        let conns = online(&[ch]);

        assert!(deposit(&db, &conns, "saver", 250));
        assert_eq!(session_gold(&conns, "saver"), (50, 250));

        flush_session(&db, &conns, "saver");
        assert_eq!(db_gold(&db, "saver"), (50, 250));
    }

    fn flush_session(db: &Db, conns: &SharedConnections, name: &str) {
        let guard = conns.lock().unwrap();
        for s in guard.values() {
            if let Some(ref c) = s.character {
                if c.name.eq_ignore_ascii_case(name) {
                    db.save_character_data(c.clone()).expect("flush");
                }
            }
        }
    }

    #[test]
    fn an_offline_character_still_works() {
        let (db, _t) = temp_db();
        saved(&db, "away", 100, 10);
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let state = World::minimal_shared(db.clone(), conns.clone());

        assert!(add_gold(&db, &conns, &state, "away", 50));
        assert_eq!(db_gold(&db, "away").0, 150);
        assert!(add_bank(&db, &conns, "away", 5));
        assert_eq!(db_gold(&db, "away").1, 15);
    }

    #[test]
    fn a_refused_change_leaves_the_balance_alone() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "poor", 10, 5);
        let conns = online(&[ch]);
        let state = World::minimal_shared(db.clone(), conns.clone());

        assert!(!add_gold(&db, &conns, &state, "poor", -11), "cannot go negative");
        assert_eq!(session_gold(&conns, "poor").0, 10);

        assert!(!add_bank(&db, &conns, "poor", -6), "cannot overdraw");
        assert_eq!(session_gold(&conns, "poor").1, 5);

        assert!(!deposit(&db, &conns, "poor", 11), "cannot deposit what you lack");
        assert!(!withdraw(&db, &conns, "poor", 6), "nor withdraw it");
        assert_eq!(session_gold(&conns, "poor"), (10, 5));
    }

    #[test]
    fn a_missing_character_is_a_refusal_not_a_panic() {
        let (db, _t) = temp_db();
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let state = World::minimal_shared(db.clone(), conns.clone());

        assert!(!add_gold(&db, &conns, &state, "nobody", 10));
        assert!(!add_bank(&db, &conns, "nobody", 10));
        assert!(!deposit(&db, &conns, "nobody", 10));
    }

    #[test]
    fn the_high_water_mark_only_rises() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "rich", 0, 0);
        let conns = online(&[ch]);
        let state = World::minimal_shared(db.clone(), conns.clone());

        assert!(add_gold(&db, &conns, &state, "rich", 900));
        let peak = db
            .get_character_data("rich")
            .expect("read")
            .expect("exists")
            .gold_high_water;
        assert_eq!(peak, 900);

        assert!(add_gold(&db, &conns, &state, "rich", -400));
        let after = db.get_character_data("rich").expect("read").expect("exists");
        assert_eq!(after.gold, 500);
        assert_eq!(after.gold_high_water, 900, "spending does not lower the peak");
    }

    /// `gold.earned` counts money arriving in the pocket, and only that.
    /// Deposits and withdrawals move a player's own coin around, so counting
    /// them would let anyone farm the counter with one gold piece.
    #[test]
    fn only_incoming_pocket_gold_counts_as_earned() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "counted", 100, 100);
        let conns = online(&[ch]);
        let state = World::minimal_shared(db.clone(), conns.clone());

        let earned = |db: &Db| -> u32 {
            db.get_character_data("counted")
                .expect("read")
                .expect("exists")
                .achievement_counters
                .get("gold.earned")
                .copied()
                .unwrap_or(0)
        };

        add_gold(&db, &conns, &state, "counted", 60);
        assert_eq!(earned(&db), 60);
        add_gold(&db, &conns, &state, "counted", -60);
        assert_eq!(earned(&db), 60, "spending is not earning");

        deposit(&db, &conns, "counted", 50);
        withdraw(&db, &conns, "counted", 50);
        assert_eq!(earned(&db), 60, "moving your own money is not earning either");
    }

    #[test]
    fn setters_clamp_rather_than_storing_a_negative_balance() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "clamped", 50, 50);
        let conns = online(&[ch]);
        let state = World::minimal_shared(db.clone(), conns.clone());

        assert!(set_gold(&db, &conns, &state, "clamped", -100));
        assert_eq!(session_gold(&conns, "clamped").0, 0);
        assert!(set_bank(&db, &conns, "clamped", -100));
        assert_eq!(session_gold(&conns, "clamped").1, 0);
    }
}
