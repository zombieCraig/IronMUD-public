//! Rhai bindings for consignment brokers.
//!
//! Every state change is a whole transaction in Rust — item move, listing
//! write, gold split, counter bump, character save — and returns a map the
//! script renders. Splitting a sale across several script-callable steps would
//! make a half-applied trade reachable from a typo in a `.rhai` file, and a
//! player market is the one place where that costs someone real value.
//!
//! The rules themselves live in [`crate::consignment`]; this module is the
//! plumbing that applies them.

use std::sync::Arc;

use rhai::{Dynamic, Engine, Map};
use uuid::Uuid;

use crate::SharedConnections;
use crate::consignment::{
    DEFAULT_LISTING_DURATION_SECS, listing_refusal, max_listings_for, price_refusal, split_proceeds,
};
use crate::db::Db;
use crate::types::{ConsignmentListing, ItemLocation};

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn result(ok: bool, message: impl Into<String>) -> Dynamic {
    let mut m = Map::new();
    m.insert("ok".into(), Dynamic::from(ok));
    m.insert("message".into(), Dynamic::from(message.into()));
    Dynamic::from(m)
}

/// One listing as a script map.
///
/// `broker` is the prototype's name, resolved here rather than left as a vnum:
/// withdrawal now has to happen at the broker, so "which broker" is the first
/// thing a seller looking at their own listings needs to know. Falls back to
/// the vnum when the prototype has been deleted, which is still more use than
/// nothing.
fn listing_map(db: &Db, listing: &ConsignmentListing, now: i64) -> Dynamic {
    let mut m = Map::new();
    m.insert("id".into(), Dynamic::from(listing.id.to_string()));
    m.insert("item_id".into(), Dynamic::from(listing.item_id.to_string()));
    m.insert("item_name".into(), Dynamic::from(listing.item_name.clone()));
    m.insert("seller".into(), Dynamic::from(listing.seller_name.clone()));
    m.insert("price".into(), Dynamic::from(listing.price));
    m.insert(
        "expires_in_secs".into(),
        Dynamic::from((listing.expires_at - now).max(0)),
    );
    let broker = db
        .get_mobile_by_vnum(&listing.broker_vnum)
        .ok()
        .flatten()
        .map(|m| m.name)
        .unwrap_or_else(|| listing.broker_vnum.clone());
    m.insert("broker".into(), Dynamic::from(broker));
    Dynamic::from(m)
}

/// Where a listed item lives while it waits for a buyer.
///
/// `ItemLocation::Consigned` would be a new variant on a widely-matched enum;
/// instead the item keeps a location nothing else resolves — a container id
/// that is the listing's own id. It is out of every room, inventory and real
/// container, so nothing can pick it up, and the listing is the only thing that
/// knows where it is. That also means an item can only ever be in one listing.
fn limbo_for(listing_id: &Uuid) -> ItemLocation {
    ItemLocation::Container(*listing_id)
}

/// Oldest listing first, ties broken by id.
///
/// Both listing views go through here because the shelf is bought from **by
/// position**, and sled hands rows back in key order — that is listing-uuid
/// order, which is to say random. A shelf that shuffles is a shelf where the
/// number a player read is not the number they buy. Oldest-first is also the
/// order a shelf ought to have: the thing that has been sitting there longest
/// is at the front.
fn shelf_order(listings: &mut [ConsignmentListing]) {
    listings.sort_by(|a, b| a.listed_at.cmp(&b.listed_at).then_with(|| a.id.cmp(&b.id)));
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: crate::SharedState) {
    // find_consignment_broker_in_room(room_id) -> MobileData or ()
    let cloned_db = db.clone();
    engine.register_fn("find_consignment_broker_in_room", move |room_id: String| -> Dynamic {
        let Ok(uuid) = Uuid::parse_str(&room_id) else {
            return Dynamic::UNIT;
        };
        let Ok(mobiles) = cloned_db.get_mobiles_in_room(&uuid) else {
            return Dynamic::UNIT;
        };
        for mobile in mobiles {
            if mobile.flags.consignment && !mobile.is_prototype {
                return Dynamic::from(mobile);
            }
        }
        Dynamic::UNIT
    });

    // consignments_at(broker_mobile_id) -> Array of listing maps
    let cloned_db = db.clone();
    engine.register_fn("consignments_at", move |broker_id: String| -> rhai::Array {
        let Some(vnum) = broker_vnum(&cloned_db, &broker_id) else {
            return rhai::Array::new();
        };
        let now = now_secs();
        let mut listings = cloned_db.get_consignments_by_broker(&vnum).unwrap_or_default();
        shelf_order(&mut listings);
        listings.iter().map(|l| listing_map(&cloned_db, l, now)).collect()
    });

    // my_consignments(char_name) -> Array of listing maps
    let cloned_db = db.clone();
    engine.register_fn("my_consignments", move |char_name: String| -> rhai::Array {
        let now = now_secs();
        let mut listings = cloned_db.get_consignments_by_seller(&char_name).unwrap_or_default();
        shelf_order(&mut listings);
        listings.iter().map(|l| listing_map(&cloned_db, l, now)).collect()
    });

    // consign_item(char_name, item_id, broker_mobile_id, price) -> #{ok, message}
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "consign_item",
            move |char_name: String, item_id: String, broker_id: String, price: i64| -> Dynamic {
                do_consign(&cloned_db, &char_name, &item_id, &broker_id, price)
            },
        );
    }

    // unconsign_item(char_name, room_id, listing_id) -> #{ok, message}
    {
        let cloned_db = db.clone();
        engine.register_fn(
            "unconsign_item",
            move |char_name: String, room_id: String, listing_id: String| -> Dynamic {
                do_unconsign(&cloned_db, &char_name, &room_id, &listing_id)
            },
        );
    }

    // buy_consignment(char_name, listing_id) -> #{ok, message}
    {
        let cloned_db = db.clone();
        let conns = connections.clone();
        let st = state.clone();
        engine.register_fn(
            "buy_consignment",
            move |char_name: String, listing_id: String| -> Dynamic {
                do_buy(&cloned_db, &conns, &st, &char_name, &listing_id)
            },
        );
    }
}

/// Resolve a broker *instance* id to the prototype vnum its shelf is keyed by.
fn broker_vnum(db: &Db, broker_id: &str) -> Option<String> {
    let uuid = Uuid::parse_str(broker_id).ok()?;
    let mob = db.get_mobile_data(&uuid).ok().flatten()?;
    if !mob.flags.consignment {
        return None;
    }
    // A broker with no vnum is a one-off instance an area reset will delete,
    // taking any shelf keyed to it. Refuse rather than lose player property.
    Some(mob.vnum).filter(|v| !v.is_empty())
}

fn do_consign(db: &Db, char_name: &str, item_id: &str, broker_id: &str, price: i64) -> Dynamic {
    let Ok(item_uuid) = Uuid::parse_str(item_id) else {
        return result(false, "You are not carrying that.");
    };
    let Ok(broker_uuid) = Uuid::parse_str(broker_id) else {
        return result(false, "There is nobody here to take it.");
    };
    let Ok(Some(broker)) = db.get_mobile_data(&broker_uuid) else {
        return result(false, "There is nobody here to take it.");
    };
    if !broker.flags.consignment {
        return result(false, format!("{} does not take goods on consignment.", broker.name));
    }
    let Some(vnum) = Some(broker.vnum.clone()).filter(|v| !v.is_empty()) else {
        return result(false, format!("{} has no shelf to put it on.", broker.name));
    };

    let Ok(Some(item)) = db.get_item_data(&item_uuid) else {
        return result(false, "You are not carrying that.");
    };
    // Ownership is checked from the item's own location, not from a passed-in
    // inventory listing: the location is the source of truth, and anything else
    // is a claim the caller could have got wrong.
    let held = matches!(&item.location, ItemLocation::Inventory(n) if n.eq_ignore_ascii_case(char_name));
    if !held {
        return result(false, "You are not carrying that.");
    }

    if let Some(refusal) = listing_refusal(&item) {
        return result(false, refusal);
    }
    if let Some(refusal) = price_refusal(item.value as i64, price) {
        return result(false, refusal);
    }

    // The broker's accepted types gate consignment as well as direct sales, so
    // a weaponsmith's shelf does not fill up with fish.
    if !crate::script::shops::shop_accepts_item(db, &broker, &item) {
        return result(false, format!("{} has no interest in that.", broker.name));
    }

    let cap = max_listings_for(broker.consignment_max_listings_per_player);
    let mine = db
        .get_consignments_by_broker(&vnum)
        .unwrap_or_default()
        .into_iter()
        .filter(|l| l.seller_name.eq_ignore_ascii_case(char_name))
        .count() as i32;
    if mine >= cap {
        return result(
            false,
            format!("You already have {} things on that shelf. Clear one first.", cap),
        );
    }

    let now = now_secs();
    let listing = ConsignmentListing::new(
        item.id,
        item.name.clone(),
        char_name.to_string(),
        vnum,
        price,
        now,
        DEFAULT_LISTING_DURATION_SECS,
    );

    let mut moved = item.clone();
    moved.location = limbo_for(&listing.id);
    if db.save_item_data(moved).is_err() {
        return result(false, "Something went wrong and the deal fell through.");
    }
    if db.save_consignment(&listing).is_err() {
        // Put it back rather than leaving the item in a limbo nothing points to.
        let mut restored = item.clone();
        restored.location = ItemLocation::Inventory(char_name.to_lowercase());
        let _ = db.save_item_data(restored);
        return result(false, "Something went wrong and the deal fell through.");
    }
    let listing_id = listing.id;
    let _ = db.update_character(char_name, |c| c.consignment_ids.push(listing_id));

    result(
        true,
        format!(
            "You hand over {} . {} sets it out at {} gold, minus {}% when it sells.",
            item.name, broker.name, price, broker.consignment_commission_pct
        ),
    )
}

/// Is a broker holding *this* listing's shelf standing in `room_id`?
///
/// Withdrawal has to happen at the counter, and it is checked here rather than
/// in `unconsign.rhai` and `consignments.rhai` separately — a placement rule
/// with two spellings is a placement rule with two behaviours. Without it the
/// shelf is a free cross-map teleport and an unlimited bag: consign ten items,
/// walk across the world, take them all back.
fn broker_present(db: &Db, room_id: &str, broker_vnum: &str) -> bool {
    let Ok(uuid) = Uuid::parse_str(room_id) else {
        return false;
    };
    let Ok(mobiles) = db.get_mobiles_in_room(&uuid) else {
        return false;
    };
    mobiles
        .iter()
        .any(|m| !m.is_prototype && m.flags.consignment && m.vnum == broker_vnum)
}

fn do_unconsign(db: &Db, char_name: &str, room_id: &str, listing_id: &str) -> Dynamic {
    let Ok(uuid) = Uuid::parse_str(listing_id) else {
        return result(false, "You have nothing like that on the shelf.");
    };
    let Ok(Some(listing)) = db.get_consignment(&uuid) else {
        return result(false, "You have nothing like that on the shelf.");
    };
    if !listing.seller_name.eq_ignore_ascii_case(char_name) {
        return result(false, "That is not yours to take back.");
    }
    if !broker_present(db, room_id, &listing.broker_vnum) {
        return result(
            false,
            format!(
                "{} is not here. You have to go back to the broker holding it.",
                listing.item_name
            ),
        );
    }

    match db.get_item_data(&listing.item_id) {
        Ok(Some(mut item)) => {
            // The same limit `get` and `buy` enforce. Without it the shelf is a
            // bag with no weight, and the item stays listed rather than being
            // dumped on someone who cannot lift it.
            if !crate::script::items::can_carry(db, char_name, &item) {
                return result(
                    false,
                    format!("You could not carry {} on top of everything else.", item.name),
                );
            }
            item.location = ItemLocation::Inventory(char_name.to_lowercase());
            let _ = db.save_item_data(item);
        }
        // The item is gone (an admin purge, a stale row). Clear the listing
        // anyway rather than leaving a row that can never be resolved.
        _ => {}
    }
    let _ = db.delete_consignment(&uuid);
    let _ = db.update_character(char_name, |c| c.consignment_ids.retain(|id| *id != uuid));

    result(true, format!("You take {} back off the shelf.", listing.item_name))
}

/// Retire a listing whose item no longer exists, and tell the seller.
///
/// A listed item can still be destroyed under the seller — food on the shelf
/// keeps spoiling, and an admin can purge anything. The buyer's apology was
/// already handled; the seller is the one who lost something and used to hear
/// nothing at all, discovering it only by finding a gap in `consignments`.
fn clear_missing_listing(db: &Db, connections: &SharedConnections, listing: &ConsignmentListing) {
    let listing_id = listing.id;
    let _ = db.delete_consignment(&listing_id);
    let _ = db.update_character(&listing.seller_name, |c| {
        c.consignment_ids.retain(|id| *id != listing_id);
    });
    crate::script::achievements::send_to_player(
        connections,
        &listing.seller_name,
        &format!(
            "\n[Consignment] {} is no longer on the shelf — there is nothing left of it to sell.\n",
            listing.item_name
        ),
    );
}

fn do_buy(
    db: &Db,
    connections: &SharedConnections,
    state: &crate::SharedState,
    char_name: &str,
    listing_id: &str,
) -> Dynamic {
    let Ok(uuid) = Uuid::parse_str(listing_id) else {
        return result(false, "That is not for sale.");
    };
    let Ok(Some(listing)) = db.get_consignment(&uuid) else {
        return result(false, "That is not for sale.");
    };
    if listing.seller_name.eq_ignore_ascii_case(char_name) {
        return result(false, "It is already yours. Take it back instead.");
    }

    // Re-validate: a listing is a claim about an item, and the sale checks the
    // claim rather than trusting a row that may be days old.
    let Ok(Some(mut item)) = db.get_item_data(&listing.item_id) else {
        clear_missing_listing(db, connections, &listing);
        return result(
            false,
            "The shelf is empty where that was. The broker looks embarrassed.",
        );
    };

    let Ok(Some(buyer)) = db.get_character_data(&char_name.to_lowercase()) else {
        return result(false, "That is not for sale.");
    };
    if (buyer.gold as i64) < listing.price {
        return result(false, format!("You cannot afford it. It is {} gold.", listing.price));
    }
    // Checked before any money moves — the same limit `get` and the ordinary
    // shop `buy` enforce. A sale that leaves the buyer overloaded, or that
    // charges them and then refuses the item, is worse than a refusal.
    if !crate::script::items::can_carry(db, char_name, &item) {
        return result(
            false,
            format!("You could not carry {} on top of everything else.", item.name),
        );
    }

    // The commission is read from the broker *prototype*, not from the
    // instance that happened to be standing there when the item was listed:
    // instances are cloned at spawn and deleted by area resets, so a rate read
    // from one would be gone by the time anything sold.
    let commission_pct = db
        .get_mobile_by_vnum(&listing.broker_vnum)
        .ok()
        .flatten()
        .map(|m| m.consignment_commission_pct)
        .unwrap_or(10);
    let (to_seller, commission) = split_proceeds(listing.price, commission_pct);

    // Charge the buyer *before* handing over the item, and through
    // `crate::purse` rather than a bare `save_character_data`. The bare version
    // was a DB-only write, so the `notify_counter_core` call at the bottom of
    // this function saved the buyer's stale session copy straight back over it
    // and the purchase refunded itself. `purse::add_gold` writes the session,
    // which is what the rest of the game reads.
    if !crate::purse::add_gold(db, connections, state, char_name, -listing.price) {
        return result(false, format!("You cannot afford it. It is {} gold.", listing.price));
    }

    item.location = ItemLocation::Inventory(char_name.to_lowercase());
    if db.save_item_data(item).is_err() {
        // Refund rather than leave them short for an item they never received.
        let _ = crate::purse::add_gold(db, connections, state, char_name, listing.price);
        return result(false, "Something went wrong and the deal fell through.");
    }

    // The seller is usually offline, so proceeds go to the bank rather than to
    // carried gold — the bank is already the "reachable from any bank or ATM"
    // account, and gold that only lands when they next log in would be gold
    // they cannot plan around. Same session rule as the buyer above: an
    // *online* seller was the one who lost the money before, because their
    // session copy outlived the DB write.
    let _ = crate::purse::add_bank(db, connections, &listing.seller_name, to_seller);
    let _ = db.update_character(&listing.seller_name, |c| {
        c.consignment_ids.retain(|id| *id != uuid);
    });
    let _ = db.delete_consignment(&uuid);

    crate::script::achievements::send_to_player(
        connections,
        &listing.seller_name,
        &format!(
            "\x1b[1;32m[ {} sold for {} gold. {} banked after the broker's cut. ]\x1b[0m",
            listing.item_name, listing.price, to_seller
        ),
    );
    // New counters, and the leaderboard picks them up with no leaderboard edit
    // at all — boards are discovered from character data.
    //
    // `gold.spent` is bumped here rather than inside `purse::add_gold` because
    // `buy`, `rent` and `identify` already bump it themselves; a helper that
    // also did would count every ordinary purchase twice. `gold.earned` is the
    // opposite case — `purse` fires it for carried gold, and this is a bank
    // credit, so the sale would otherwise never count as income at all.
    crate::script::achievements::notify_counter_core(db, connections, state, &listing.seller_name, "items.sold", 1);
    crate::script::achievements::notify_counter_core(db, connections, state, char_name, "items.bought", 1);
    if listing.price > 0 {
        crate::script::achievements::notify_counter_core(
            db,
            connections,
            state,
            char_name,
            "gold.spent",
            listing.price.min(u32::MAX as i64) as u32,
        );
    }
    if to_seller > 0 {
        crate::script::achievements::notify_counter_core(
            db,
            connections,
            state,
            &listing.seller_name,
            "gold.earned",
            to_seller.min(u32::MAX as i64) as u32,
        );
    }

    result(
        true,
        format!(
            "You buy {} for {} gold. ({} gold of it went to the broker.)",
            listing.item_name, listing.price, commission
        ),
    )
}
