# Consignment Brokers

A consignment broker is an NPC that holds goods for other players. A seller
leaves an item at a price of their choosing; any other player can buy it from
that broker; the proceeds land in the seller's bank whether or not they are
online.

This is IronMUD's player-to-player economy. It is the only system where two
players can move value between themselves without standing in the same room, so
it is worth understanding what the guard rails do before you place one.

## Making a broker

```
medit <vnum> flag consignment on
medit <vnum> shop commission 10
medit <vnum> shop listingcap 0
```

| Field | Default | Meaning |
|-------|---------|---------|
| `consignment` flag | off | The mob accepts consignments |
| `shop commission <0-100>` | 10 | Percentage the broker keeps from each sale |
| `shop listingcap <n>` | 0 | Listings one player may hold here. `0` uses the shared default of 10 |

The `consignment` flag is **independent of `shopkeeper`**. A broker can be a
pure middleman with no stock of its own, or a shopkeeper that also runs a
consignment shelf.

`list` builds one page and one numbering. Shop stock (or vending stock) comes
first; the shelf is appended and continues the same numbers, so `buy 7` means
whatever `list` printed as 7 whether that was the shopkeeper's own goods or a
player's. This also means a broker standing **next to** an unrelated shopkeeper
still works — the shelf is appended to that merchant's page rather than being
hidden behind it.

### The broker needs a vnum

**A broker without a prototype vnum will refuse every consignment.** The shelf
is keyed by prototype vnum, not by the instance standing in the room, because
mobile instances are cloned at spawn and deleted by area resets. An id-keyed
shelf would empty itself the first time the zone reset, taking every listed
item with it. A one-off instance with no vnum has no shelf to key, so the
broker says so rather than accepting goods it will lose.

### What a broker will take

Consignment reuses the shop's accepted-types filter. `shop buys`,
`shop categories`, `shop preset`, `shop minvalue` and `shop maxvalue` all apply
to the shelf exactly as they apply to the counter, so a weaponsmith's shelf
does not fill up with fish. A broker that buys nothing accepts nothing on
consignment either — set `shop buys all` on a general-purpose broker.

## The guard rails, and why each exists

These are enforced in `src/consignment.rs` and are not builder-tunable, because
they are what stop the market becoming a laundering channel.

| Rule | Why |
|------|-----|
| Price must be at least **25% of the item's value** | Listing a 500-gold sword for 1 gold is a gold hand-off wearing a shop's clothes |
| Price may be at most **10× the item's value** | Ten times a fair price is not a price; it is a mule trade dressed as commerce |
| Items with `value: 0` get a floor of 1 gold and no ceiling | Builders leave `value` at 0 constantly; a hard refusal would make half the world unlistable |
| Corpses, quest items, `no_drop` items and gold are refused | Each routes around a rule that exists elsewhere — see below |
| The commission is destroyed, not paid to anyone | A player economy with no sink inflates until prices mean nothing |
| Listings are validated again at sale time | A listing is a claim about an item; the sale checks the claim rather than trusting a row that may be days old |

The refusal list in detail:

- **Corpses** are containers of someone else's loot, on a decay timer, with
  their own protection rules. A shelf routes around all three.
- **Quest items** are bookkeeping for a quest the buyer is probably not on;
  selling one can strand its owner's objective with no way to recover it.
- **`no_drop` items** are bound by explicit builder intent, and consignment is
  a drop with extra steps.
- **Gold** listed for gold is a wash trade with a commission attached.

## Lifecycle

1. `consign <item> <price>` moves the item out of the seller's inventory into
   the listing. It exists in exactly one place until it resolves.
2. Any other player at that broker sees it under `list` and takes it with
   `buy <number>`.
3. On sale the buyer pays the full price; the seller banks the price minus
   commission; the commission is destroyed. Both sides get an `items.sold` /
   `items.bought` counter bump, which means each grows a `top` board with no
   leaderboard code change at all.
4. `unconsign <number>` or `consignments take <number>` pulls a listing back —
   **at the broker holding it**. Withdrawal is a counter transaction, not a
   remote one; otherwise the shelf is a free courier between areas and a bag
   with no carry weight. `consignments` names the broker for each listing so a
   seller knows where to go.
5. **After seven days an unsold listing moves to escrow**, not to deletion.
   Escrow is already "items the game is holding for a named player, with a
   deletion date", so a mispriced item is recoverable rather than destroyed.
   The retrieval fee is zero — the seller already paid for the attempt by
   having the goods off the market for a week.

## Placing one

A broker wants to be somewhere players pass through: a market square, an inn, a
guild hall. Two brokers in different areas do **not** share a shelf — each
prototype vnum has its own — so a second broker is a second market, not a
second door onto the first. That is usually what you want for a regional
economy and a mistake if you meant one central exchange; in that case, use one
vnum and place multiple instances of it, which *do* share the shelf.

## Player commands

| Command | Description |
|---------|-------------|
| `consign <item> <price>` | Leave an item with the broker in this room |
| `list` | See the shelf (and the broker's own stock, if it has any) |
| `buy <number>` | Buy a listing by its position in `list` |
| `consignments` | Your own listings, with prices and time to expiry |
| `consignments take <n>` / `unconsign <n>` | Take one back, standing at that broker |

Buying and withdrawing are by list position rather than by keyword on purpose:
two players can have the same item out at two prices, and a keyword cannot say
which one was meant.

## Placing one, revisited

A broker beside a shopkeeper is fine and is often the best spot — the two share
one `list` page. What a broker cannot do is hold goods it will lose: give it a
prototype vnum, and give it a `shop buys` filter wide enough to accept what you
want players to trade there.
