//! Party membership, and who a kill belongs to.
//!
//! Two jobs live here because they are the same question asked twice:
//!
//! 1. **Who is in a party** — resolved from the live session table
//!    (`following` + `is_grouped`), never from the database. Session copies are
//!    authoritative for anyone online, and a party is by definition made of
//!    online players.
//! 2. **Who a mob kill credits** — the contributor set the kill block fans its
//!    consumers over.
//!
//! Before this module the contributor set was computed inside
//! `quest::handle_mob_kill` and nowhere else, so quests credited the whole
//! party while the kill counter, worship favor, morality and faction standing
//! credited only the killing blow. One set, one rule, six consumers.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::SharedConnections;

/// The set of characters a single mob kill credits.
///
/// `killer` is the killing blow and always appears in `participants` too —
/// consumers that treat everyone alike just walk `participants`, and only the
/// presentation layer needs to tell the killer apart (they get the
/// second-person line; everyone else gets the party form).
#[derive(Debug, Clone)]
pub struct KillCredit {
    pub killer: String,
    /// Lowercased, deduplicated, and stable-ordered with the killer first so
    /// the resolution messages arrive in a predictable order.
    pub participants: Vec<String>,
}

impl KillCredit {
    /// True when this kill is a solo one. Lets a caller skip the party wording
    /// entirely rather than testing lengths at each site.
    pub fn is_solo(&self) -> bool {
        self.participants.len() <= 1
    }

    /// Everyone except the killing blow, in `participants` order.
    pub fn assistants(&self) -> impl Iterator<Item = &String> {
        let killer = self.killer.to_lowercase();
        self.participants.iter().filter(move |n| **n != killer)
    }
}

/// Walk the `following` chain from `char_name` to the party's ultimate leader.
/// Returns an empty string when the character follows nobody (they are the
/// leader, or are unattached). Cycle-safe.
///
/// Mirrors what the `get_group_leader` Rhai binding used to do inline; that
/// binding now calls this so there is one implementation of the chain walk.
pub fn group_leader(connections: &SharedConnections, char_name: &str) -> String {
    let Ok(conns) = connections.lock() else {
        return String::new();
    };
    let char_lower = char_name.to_lowercase();

    let following_of = |name: &str| -> Option<String> {
        let lower = name.to_lowercase();
        conns.values().find_map(|s| {
            s.character
                .as_ref()
                .filter(|c| c.name.to_lowercase() == lower)
                .and_then(|c| c.following.clone())
        })
    };

    let mut leader = match following_of(&char_lower) {
        Some(name) => name,
        None => return String::new(),
    };

    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(char_lower);
    loop {
        let leader_lower = leader.to_lowercase();
        if !visited.insert(leader_lower.clone()) {
            // Cycle: stop where we are rather than spinning.
            return leader;
        }
        match following_of(&leader_lower) {
            Some(next) => leader = next,
            None => return leader,
        }
    }
}

/// Grouped members whose party ultimately leads back to `leader_name`, and who
/// are currently in `room_id`. Does **not** include the leader —
/// [`party_in_room`] is the function that answers "the whole party, leader
/// included".
///
/// Membership is resolved by walking each candidate's own follow chain to its
/// end, not by comparing a single hop. [`group_leader`] has always been
/// transitive, and when this was one-hop the two disagreed: in a chain
/// A ← B ← C, C resolved A as their leader while A's membership list contained
/// only B. That cost C their kill credit and left them out of their own group
/// panel — the same "support contributed and earned nothing" defect the
/// party-credit work exists to fix, surviving one level deeper.
pub fn group_members_in_room(connections: &SharedConnections, leader_name: &str, room_id: &Uuid) -> Vec<String> {
    let candidates: Vec<String> = {
        let Ok(conns) = connections.lock() else {
            return Vec::new();
        };
        conns
            .values()
            .filter_map(|s| {
                let c = s.character.as_ref()?;
                (c.is_grouped && c.current_room_id == *room_id && c.following.is_some()).then(|| c.name.clone())
            })
            .collect()
    };

    // `group_leader` takes the lock itself, so the candidate scan above has to
    // finish first — `std::sync::Mutex` is not reentrant.
    candidates
        .into_iter()
        .filter(|name| group_leader(connections, name).eq_ignore_ascii_case(leader_name))
        .collect()
}

/// Every member of `char_name`'s party who is in `room_id`, including
/// `char_name` and the leader. Names are lowercased and deduplicated.
///
/// Presence is part of the definition on purpose: a grouped player standing in
/// another zone did not take part in the fight, and crediting them would make
/// grouping a way to farm standing from a safe room.
pub fn party_in_room(connections: &SharedConnections, char_name: &str, room_id: &Uuid) -> Vec<String> {
    // A tagalong is not a party member. `in_same_party` has always said so —
    // "merely following someone is not enough" — but this function used to
    // resolve a leader from `following` alone, so someone who typed `follow`
    // and was never added with `group` handed their kill credit to a party
    // that never accepted them. One rule, asked in both places.
    //
    // A leader is exempt: they are trivially in their own party and do not
    // carry `is_grouped` themselves.
    if !is_grouped_or_leader(connections, char_name) {
        return vec![char_name.to_lowercase()];
    }

    let leader = group_leader(connections, char_name);
    let leader = if leader.is_empty() {
        char_name.to_string()
    } else {
        leader
    };

    let mut out: Vec<String> = Vec::new();
    let mut push = |name: String| {
        let lower = name.to_lowercase();
        if !out.contains(&lower) {
            out.push(lower);
        }
    };

    // The leader only counts if they are actually here.
    if let Ok(conns) = connections.lock() {
        let leader_lower = leader.to_lowercase();
        let leader_here = conns.values().any(|s| {
            s.character
                .as_ref()
                .is_some_and(|c| c.name.to_lowercase() == leader_lower && c.current_room_id == *room_id)
        });
        if leader_here {
            push(leader.clone());
        }
    }
    for member in group_members_in_room(connections, &leader, room_id) {
        push(member);
    }
    push(char_name.to_string());
    out
}

/// Does this character belong to a party at all — either as someone deliberately
/// added with `group` (`is_grouped`), or as a leader, who follows nobody and is
/// trivially in their own?
///
/// Someone who typed `follow` and was never grouped is neither, and is a
/// spectator for every purpose in this module.
fn is_grouped_or_leader(connections: &SharedConnections, char_name: &str) -> bool {
    let Ok(conns) = connections.lock() else {
        return false;
    };
    conns.values().any(|s| {
        s.character
            .as_ref()
            .is_some_and(|c| c.name.eq_ignore_ascii_case(char_name) && (c.is_grouped || c.following.is_none()))
    })
}

/// Are `a` and `b` in the same party right now?
///
/// Both resolve to the same ultimate leader, **and** whichever of them is not
/// that leader carries `is_grouped`. Merely following someone is not enough:
/// a tagalong who was never added to the group is a spectator, and the whole
/// point of `group <name>` is that it is a deliberate act.
///
/// Room-independent, unlike [`party_in_room`] — the caller that needs this
/// (corpse loot protection) is asking about a relationship, not about who was
/// present for a fight.
pub fn in_same_party(connections: &SharedConnections, a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let leader_a = group_leader(connections, a);
    let leader_a = if leader_a.is_empty() { a.to_string() } else { leader_a };
    let leader_b = group_leader(connections, b);
    let leader_b = if leader_b.is_empty() { b.to_string() } else { leader_b };
    if !leader_a.eq_ignore_ascii_case(&leader_b) {
        return false;
    }

    let Ok(conns) = connections.lock() else {
        return false;
    };
    let grouped = |name: &str| -> bool {
        if name.eq_ignore_ascii_case(&leader_a) {
            return true; // the leader is trivially in their own party
        }
        conns.values().any(|s| {
            s.character
                .as_ref()
                .is_some_and(|c| c.name.eq_ignore_ascii_case(name) && c.is_grouped)
        })
    };
    grouped(a) && grouped(b)
}

/// Resolve who a mob kill credits.
///
/// Three sources, unioned:
/// - everyone in `damaged_by` with non-zero damage,
/// - the killing blow,
/// - the killer's party members present in `room_id`.
///
/// That third term is the design call. `damaged_by` only knows about damage, so
/// a healer or a buffer who carried the fight contributes nothing to it — a
/// credit rule built on damage alone tells every support build it does not
/// exist. Presence plus `is_grouped` is what makes support playable.
///
/// The killer is always first in `participants`; the rest keep `damaged_by`'s
/// iteration order followed by party order, which is arbitrary but stable
/// within a single kill.
pub fn kill_credit(
    connections: &SharedConnections,
    damaged_by: &HashMap<String, i32>,
    killer: &str,
    room_id: &Uuid,
) -> KillCredit {
    let killer_lower = killer.to_lowercase();
    let mut participants: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if name.is_empty() {
            return;
        }
        let lower = name.to_lowercase();
        if !participants.contains(&lower) {
            participants.push(lower);
        }
    };

    push(killer_lower.clone());
    let mut damagers: Vec<(&String, &i32)> = damaged_by.iter().filter(|(_, d)| **d > 0).collect();
    // HashMap order is not stable across runs; sort so the messages a party
    // receives do not shuffle between otherwise identical kills.
    damagers.sort_by(|a, b| a.0.cmp(b.0));
    for (name, _) in damagers {
        push(name.clone());
    }
    if !killer.is_empty() {
        for member in party_in_room(connections, killer, room_id) {
            push(member);
        }
    }

    KillCredit {
        killer: killer_lower,
        participants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlayerSession, types::CharacterData};
    use std::collections::HashMap as Map;
    use std::sync::{Arc, Mutex};

    fn conns_with(chars: Vec<CharacterData>) -> SharedConnections {
        let mut map = Map::new();
        for c in chars {
            let (tx_client, _rx_client) = tokio::sync::mpsc::unbounded_channel::<String>();
            let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
            let mut session = PlayerSession::new_for_test(tx_client, tx_input);
            session.character = Some(c);
            map.insert(Uuid::new_v4(), session);
        }
        Arc::new(Mutex::new(map))
    }

    fn player(name: &str, room: Uuid, following: Option<&str>, grouped: bool) -> CharacterData {
        let mut c: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": room,
        }))
        .expect("build character");
        c.following = following.map(|s| s.to_string());
        c.is_grouped = grouped;
        c
    }

    /// Membership walks the whole follow chain, because `group_leader` always
    /// has. With a one-hop test, a second-level follower in an A <- B <- C chain
    /// resolved A as their leader while A's membership list held only B — so C
    /// earned nothing from a kill they were standing in, which is the exact
    /// defect party credit exists to fix, one level deeper.
    #[test]
    fn a_second_level_follower_is_still_in_the_party() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Alpha", room, None, false),
            player("Bravo", room, Some("Alpha"), true),
            player("Charlie", room, Some("Bravo"), true),
        ]);

        let members = group_members_in_room(&conns, "Alpha", &room);
        assert!(members.iter().any(|n| n == "Charlie"), "{members:?}");

        // Either end of the chain landing the blow credits all three.
        for killer in ["Alpha", "Charlie"] {
            let credit = kill_credit(&conns, &HashMap::new(), killer, &room);
            for who in ["alpha", "bravo", "charlie"] {
                assert!(
                    credit.participants.contains(&who.to_string()),
                    "{killer}'s kill should credit {who}: {:?}",
                    credit.participants
                );
            }
        }
    }

    /// `in_same_party` has always said that merely following is not enough.
    /// `party_in_room` used to resolve a leader from `following` alone, so
    /// someone who typed `follow` and was never added with `group` handed their
    /// kill credit to a party that never accepted them.
    #[test]
    fn an_ungrouped_tagalong_credits_only_themselves() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Leader", room, None, false),
            player("Member", room, Some("Leader"), true),
            player("Pup", room, Some("Leader"), false),
        ]);

        let credit = kill_credit(&conns, &HashMap::new(), "Pup", &room);
        assert_eq!(credit.participants, vec!["pup".to_string()], "a spectator kills alone");

        // And the party does not gain them either, which was already true and
        // must stay true.
        let theirs = kill_credit(&conns, &HashMap::new(), "Leader", &room);
        assert!(
            !theirs.participants.contains(&"pup".to_string()),
            "{:?}",
            theirs.participants
        );
        assert!(theirs.participants.contains(&"member".to_string()));
    }

    #[test]
    fn a_solo_killer_credits_only_themselves() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![player("Solo", room, None, false)]);
        let credit = kill_credit(&conns, &HashMap::new(), "Solo", &room);
        assert_eq!(credit.participants, vec!["solo".to_string()]);
        assert!(credit.is_solo());
    }

    /// The reason the party term exists: a healer lands no blow all fight and
    /// contributes nothing to `damaged_by`, but the kill is as much theirs.
    #[test]
    fn a_grouped_healer_who_dealt_no_damage_is_credited() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Tank", room, None, false),
            player("Medic", room, Some("Tank"), true),
        ]);
        let mut damaged = HashMap::new();
        damaged.insert("tank".to_string(), 40);

        let credit = kill_credit(&conns, &damaged, "Tank", &room);
        assert_eq!(credit.killer, "tank");
        assert!(credit.participants.contains(&"medic".to_string()));
        assert_eq!(credit.assistants().cloned().collect::<Vec<_>>(), vec!["medic"]);
    }

    #[test]
    fn an_ungrouped_bystander_in_the_room_is_not_credited() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Tank", room, None, false),
            // Following but never added to the group: watching, not fighting.
            player("Tagalong", room, Some("Tank"), false),
        ]);
        let credit = kill_credit(&conns, &HashMap::new(), "Tank", &room);
        assert_eq!(credit.participants, vec!["tank".to_string()]);
    }

    #[test]
    fn a_grouped_member_in_another_room_is_not_credited() {
        let room = Uuid::new_v4();
        let elsewhere = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Tank", room, None, false),
            player("Absent", elsewhere, Some("Tank"), true),
        ]);
        let credit = kill_credit(&conns, &HashMap::new(), "Tank", &room);
        assert_eq!(credit.participants, vec!["tank".to_string()]);
    }

    /// An ungrouped player who actually hit the mob still earns credit —
    /// `damaged_by` is a contribution record, not a party roster.
    #[test]
    fn an_ungrouped_damager_is_credited() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![player("Tank", room, None, false)]);
        let mut damaged = HashMap::new();
        damaged.insert("stranger".to_string(), 12);
        let credit = kill_credit(&conns, &damaged, "Tank", &room);
        assert!(credit.participants.contains(&"stranger".to_string()));
    }

    #[test]
    fn zero_damage_entries_do_not_count() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![player("Tank", room, None, false)]);
        let mut damaged = HashMap::new();
        damaged.insert("whiffed".to_string(), 0);
        let credit = kill_credit(&conns, &damaged, "Tank", &room);
        assert_eq!(credit.participants, vec!["tank".to_string()]);
    }

    #[test]
    fn the_killer_leads_the_list_and_appears_once() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Tank", room, None, false),
            player("Medic", room, Some("Tank"), true),
        ]);
        let mut damaged = HashMap::new();
        damaged.insert("tank".to_string(), 40);
        damaged.insert("medic".to_string(), 3);

        let credit = kill_credit(&conns, &damaged, "Tank", &room);
        assert_eq!(credit.participants[0], "tank");
        assert_eq!(credit.participants.iter().filter(|n| *n == "tank").count(), 1);
        assert_eq!(credit.participants.len(), 2);
    }

    /// A follower killing while their leader stands beside them credits the
    /// leader too — the party is resolved from whoever struck, not only from a
    /// leader who happened to swing.
    #[test]
    fn a_member_killing_credits_the_leader_present_in_the_room() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Boss", room, None, false),
            player("Grunt", room, Some("Boss"), true),
        ]);
        let credit = kill_credit(&conns, &HashMap::new(), "Grunt", &room);
        assert_eq!(credit.killer, "grunt");
        assert!(credit.participants.contains(&"boss".to_string()));
    }

    #[test]
    fn the_leader_chain_resolves_through_an_intermediate_follower() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Top", room, None, false),
            player("Middle", room, Some("Top"), true),
            player("Bottom", room, Some("Middle"), true),
        ]);
        assert_eq!(group_leader(&conns, "Bottom"), "Top");
    }

    #[test]
    fn a_following_cycle_terminates() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("A", room, Some("B"), true),
            player("B", room, Some("A"), true),
        ]);
        // Any answer will do; not hanging is the assertion.
        let leader = group_leader(&conns, "A");
        assert!(!leader.is_empty());
    }

    #[test]
    fn a_leader_and_their_grouped_member_share_a_party() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Tank", room, None, false),
            player("Medic", room, Some("Tank"), true),
        ]);
        assert!(in_same_party(&conns, "Tank", "Medic"));
        assert!(in_same_party(&conns, "Medic", "Tank"), "the test is symmetric");
    }

    /// Following without being added to the group is spectating. Corpse loot
    /// protection turns on this distinction: otherwise anyone could `follow`
    /// a corpse's owner and walk straight through the window.
    #[test]
    fn merely_following_is_not_being_in_the_party() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Tank", room, None, false),
            player("Tagalong", room, Some("Tank"), false),
        ]);
        assert!(!in_same_party(&conns, "Tank", "Tagalong"));
    }

    #[test]
    fn two_grouped_members_of_one_leader_share_a_party() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Boss", room, None, false),
            player("One", room, Some("Boss"), true),
            player("Two", room, Some("Boss"), true),
        ]);
        assert!(in_same_party(&conns, "One", "Two"));
    }

    #[test]
    fn separate_parties_do_not_mix() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("BossA", room, None, false),
            player("MemberA", room, Some("BossA"), true),
            player("BossB", room, None, false),
            player("MemberB", room, Some("BossB"), true),
        ]);
        assert!(!in_same_party(&conns, "MemberA", "MemberB"));
        assert!(!in_same_party(&conns, "BossA", "BossB"));
    }

    /// A party is only meaningful between two live sessions. Being asked about
    /// an offline name must not accidentally match another unattached player,
    /// which a naive "both resolve to themselves" comparison would do.
    #[test]
    fn two_unattached_players_are_not_a_party() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![
            player("Loner", room, None, false),
            player("Stranger", room, None, false),
        ]);
        assert!(!in_same_party(&conns, "Loner", "Stranger"));
    }

    #[test]
    fn a_character_is_always_in_their_own_party() {
        let room = Uuid::new_v4();
        let conns = conns_with(vec![player("Solo", room, None, false)]);
        assert!(in_same_party(&conns, "Solo", "solo"), "and case does not matter");
    }
}
