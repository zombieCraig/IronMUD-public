//! Consignment brokers — the rules of the player market.
//!
//! Every decision that could be exploited lives here as a named, tested
//! function rather than inline in a command script. A player market is the one
//! system in the game where two players can move value between themselves, so
//! the interesting question is never "does it work" but "what does it let
//! someone do that they should not".
//!
//! The defences, and what each is for:
//!
//! - **A price floor and ceiling relative to the item's value.** Listing a
//!   500-gold sword for 1 gold is a hand-off, not a sale; listing a 1-gold
//!   trinket for 500,000 is a mule trade dressed as commerce. Neither is
//!   forbidden outright — players do favours — but neither may masquerade as a
//!   market price, and the commission below makes both cost something.
//! - **Commission is the sink.** Every sale destroys gold. A player economy
//!   with no sink inflates until prices mean nothing; this is the one thing
//!   standing against that.
//! - **A refusal list.** Corpses, quest items, and no-drop items never reach a
//!   shelf. Each for its own reason, spelled out at [`listing_refusal`].
//! - **A per-player cap**, so one seller cannot wallpaper a broker.
//! - **Re-validation at sale time.** A listing is a claim about an item; the
//!   sale checks the claim again rather than trusting it.

use crate::types::{ItemData, ItemType};

/// How long an unsold listing sits before it expires into escrow.
pub const DEFAULT_LISTING_DURATION_SECS: i64 = 7 * 24 * 60 * 60;

/// Listings one player may have on one broker when the broker sets no limit.
pub const DEFAULT_MAX_LISTINGS_PER_PLAYER: i32 = 10;

/// A listing may not be priced below this fraction of the item's value, in
/// percent. Below it, the "sale" is a gold hand-off wearing a shop's clothes.
pub const MIN_PRICE_PCT_OF_VALUE: i64 = 25;

/// Nor above this multiple of it. Ten times a fair price is not a price.
pub const MAX_PRICE_MULTIPLE_OF_VALUE: i64 = 10;

/// The floor an item with no value of its own still has, so a `value: 0`
/// prototype cannot be listed for nothing and used as a free courier.
pub const MIN_ABSOLUTE_PRICE: i64 = 1;

/// The price band a given item may be listed within, as `(min, max)`.
///
/// An item whose `value` is zero has no market anchor, so it gets a flat
/// permissive band rather than a band computed from nothing: builders leave
/// `value` at 0 constantly and a hard refusal there would make half the world
/// unlistable.
pub fn price_band(item_value: i64) -> (i64, i64) {
    if item_value <= 0 {
        return (MIN_ABSOLUTE_PRICE, i64::MAX);
    }
    let min = ((item_value * MIN_PRICE_PCT_OF_VALUE) / 100).max(MIN_ABSOLUTE_PRICE);
    (min, item_value * MAX_PRICE_MULTIPLE_OF_VALUE)
}

/// Why this price is not allowed, or `None` when it is.
pub fn price_refusal(item_value: i64, price: i64) -> Option<String> {
    let (min, max) = price_band(item_value);
    if price < min {
        return Some(format!(
            "The broker will not list it that cheap. The least they will take is {} gold.",
            min
        ));
    }
    if price > max {
        return Some(format!(
            "The broker laughs. Nobody will pay that. The most they will list it for is {} gold.",
            max
        ));
    }
    None
}

/// Why this item may not be consigned, or `None` when it may.
///
/// Each refusal is a distinct hazard, not a taste:
///
/// - A **corpse** is a container of someone else's loot, on a decay timer, with
///   its own protection rules. Putting one on a shelf routes around all three.
/// - A **quest item** is bookkeeping for a quest the buyer is probably not on;
///   selling it can strand its owner's objective with no way to recover it.
/// - A **no-drop** item is bound by explicit builder intent. Consignment is a
///   drop with extra steps, and it must not be the way around that intent.
/// - **Gold** is currency; listing it for gold is a wash trade with a
///   commission attached, which is only ever a way to test the sink.
pub fn listing_refusal(item: &ItemData) -> Option<&'static str> {
    if item.flags.is_corpse {
        return Some("The broker will not take a body.");
    }
    if item.item_type == ItemType::Gold {
        return Some("The broker deals in goods, not in coin for coin.");
    }
    if item.flags.no_drop {
        return Some("It is bound to you. You could not hand it over if you tried.");
    }
    if item.flags.quest_item {
        return Some("The broker turns it over once and pushes it back. \"Not this. Someone is owed this.\"");
    }
    None
}

/// Split a sale price into what the seller banks and what the broker destroys.
///
/// Rounds the commission **down**, so the rounding error goes to the player
/// rather than to the sink. Clamps the percentage rather than trusting it: the
/// write surfaces validate it, but a hand-edited database should still not be
/// able to mint gold with a negative commission.
pub fn split_proceeds(price: i64, commission_pct: i32) -> (i64, i64) {
    let pct = commission_pct.clamp(0, 100) as i64;
    let commission = (price.max(0) * pct) / 100;
    (price.max(0) - commission, commission)
}

/// Listings one player may hold on a broker, given that broker's setting.
/// Zero or negative means the broker did not choose, so the shared default
/// applies.
pub fn max_listings_for(broker_setting: i32) -> i32 {
    if broker_setting > 0 {
        broker_setting
    } else {
        DEFAULT_MAX_LISTINGS_PER_PLAYER
    }
}

/// Move unsold consignment listings into escrow.
///
/// Escrow is already exactly the right shape — items the game is holding for a
/// named player, with a retrieval fee and a deletion date — so an expired
/// listing joins that queue instead of being destroyed. A player who mispriced
/// something and logged off for a fortnight gets it back at a cost, which is
/// the same deal an evicted tenant gets.
pub fn process_expired_consignments(
    db: &crate::db::Db,
    connections: &crate::SharedConnections,
    now: i64,
) -> anyhow::Result<()> {
    let expiry_days: i64 = db
        .get_setting_or_default("escrow_expiry_real_days", "30")
        .unwrap_or_else(|_| "30".to_string())
        .parse::<i64>()
        .unwrap_or(30);

    for listing in db.list_all_consignments()? {
        if !listing.is_expired(now) {
            continue;
        }

        // No retrieval fee: the seller already paid for the attempt by having
        // the item off the market for a week. Charging twice for one mistake
        // is what makes players stop using a market.
        let escrow = crate::types::EscrowData::new(
            listing.seller_name.clone(),
            vec![listing.item_id],
            uuid::Uuid::nil(),
            expiry_days,
            0,
        );
        let escrow_id = escrow.id;
        if let Err(e) = db.save_escrow(&escrow) {
            tracing::error!("Failed to escrow expired consignment {}: {}", listing.id, e);
            continue;
        }

        // The item leaves listing-limbo for escrow-limbo. Escrow addresses
        // items by id and does not read their location, so pointing it at the
        // escrow row keeps the invariant that a limbo item is reachable from
        // exactly one place.
        if let Ok(Some(mut item)) = db.get_item_data(&listing.item_id) {
            item.location = crate::types::ItemLocation::Container(escrow_id);
            let _ = db.save_item_data(item);
        }

        let listing_id = listing.id;
        if let Err(e) = db.update_character(&listing.seller_name, |c| {
            c.consignment_ids.retain(|id| *id != listing_id);
            c.escrow_ids.push(escrow_id);
        }) {
            tracing::error!("Failed to update seller after consignment expiry: {}", e);
        }
        let _ = db.delete_consignment(&listing.id);

        crate::script::achievements::send_to_player(
            connections,
            &listing.seller_name,
            &format!(
                "\n[Consignment] {} went unsold and has been moved to escrow.\n",
                listing.item_name
            ),
        );
        tracing::debug!(
            "Consignment {} expired to escrow for {}",
            listing.id,
            listing.seller_name
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ItemData;

    fn item(value: i32) -> ItemData {
        let mut i = ItemData::new("Thing".into(), "a thing".into(), String::new());
        i.value = value;
        i
    }

    #[test]
    fn the_band_is_a_quarter_of_value_up_to_ten_times_it() {
        assert_eq!(price_band(400), (100, 4000));
        assert!(price_refusal(400, 100).is_none());
        assert!(price_refusal(400, 4000).is_none());
        assert!(price_refusal(400, 99).is_some());
        assert!(price_refusal(400, 4001).is_some());
    }

    /// The laundering case the floor exists for: a 500-gold sword listed at 1
    /// gold is a gold transfer, not a sale.
    #[test]
    fn a_giveaway_price_is_refused_and_says_the_floor() {
        let refusal = price_refusal(500, 1).expect("refused");
        assert!(refusal.contains("125"), "names the actual floor: {refusal}");
    }

    #[test]
    fn a_valueless_item_still_cannot_be_listed_for_nothing() {
        assert!(price_refusal(0, 0).is_some());
        assert!(price_refusal(0, 1).is_none());
        // No ceiling to anchor to, so none is imposed.
        assert!(price_refusal(0, 1_000_000).is_none());
    }

    /// Small values must not floor to zero, or the cheapest items become free
    /// couriers.
    #[test]
    fn the_floor_never_rounds_down_to_free() {
        assert_eq!(price_band(1), (1, 10));
        assert_eq!(price_band(3), (1, 30));
        assert!(price_refusal(3, 0).is_some());
    }

    #[test]
    fn commission_is_taken_off_the_top_and_rounds_toward_the_seller() {
        assert_eq!(split_proceeds(100, 10), (90, 10));
        // 10% of 95 is 9.5; the half-coin goes to the player.
        assert_eq!(split_proceeds(95, 10), (86, 9));
        assert_eq!(split_proceeds(100, 0), (100, 0));
        assert_eq!(split_proceeds(100, 100), (0, 100));
    }

    /// A hand-edited commission must not be able to mint gold.
    #[test]
    fn an_out_of_range_commission_is_clamped_rather_than_trusted() {
        assert_eq!(split_proceeds(100, -50), (100, 0));
        assert_eq!(split_proceeds(100, 500), (0, 100));
    }

    #[test]
    fn proceeds_never_go_negative() {
        assert_eq!(split_proceeds(-10, 10), (0, 0));
    }

    #[test]
    fn corpses_quest_items_bound_items_and_coin_are_all_refused() {
        let mut corpse = item(10);
        corpse.flags.is_corpse = true;
        assert!(listing_refusal(&corpse).is_some());

        let mut bound = item(10);
        bound.flags.no_drop = true;
        assert!(listing_refusal(&bound).is_some());

        let mut quest = item(10);
        quest.flags.quest_item = true;
        assert!(listing_refusal(&quest).is_some());

        let mut coin = item(10);
        coin.item_type = ItemType::Gold;
        assert!(listing_refusal(&coin).is_some());

        assert!(listing_refusal(&item(10)).is_none(), "an ordinary item is fine");
    }

    #[test]
    fn a_broker_that_sets_no_cap_gets_the_shared_default() {
        assert_eq!(max_listings_for(0), DEFAULT_MAX_LISTINGS_PER_PLAYER);
        assert_eq!(max_listings_for(-3), DEFAULT_MAX_LISTINGS_PER_PLAYER);
        assert_eq!(max_listings_for(2), 2);
    }
}
